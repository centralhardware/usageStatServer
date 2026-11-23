//! Records which app is in front of the Mac (with its window title) and when nobody is at it,
//! and POSTs the blocks to UsageStatServer. Runs resident as a per-user LaunchAgent (see
//! packaging/): it needs the user's GUI session to see the frontmost app.
//!
//! Every second it samples the frontmost app (NSWorkspace), the focused window's title
//! (Accessibility, asked for on first run; titles stay empty until granted), input idle time
//! and the lock state; tracker.rs turns that into blocks. Every 60 s it sends the queued
//! blocks plus the open one (re-sent with a later stop each time).
//!
//! Server URL: $USAGE_AGENT_URL, else `ServerURL` in
//! ~/Library/Preferences/me.centralhardware.usage-agent.plist (the installer asks for it),
//! plus an optional `AuthToken` ($USAGE_AGENT_TOKEN) sent as a bearer token.
//! Preferences are re-read every flush, so changes need no restart.
//! Every 10 s it also checks what plays video (the app holding a display-sleep power
//! assertion) and, for a browser, the front tab's domain via AppleScript (macOS asks once per
//! browser for Automation); those go to `mac_media`.
//! `--dry-run` prints one observation and exits.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod screentime;

use std::process::ExitCode;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("usage-agent only runs on macOS");
    ExitCode::FAILURE
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use usage_agent::queue::{Item, Queue, SendError};
    use usage_agent::tracker::{MediaTracker, Observation, SLEEP_GAP_MS, Tracker};

    const ENDPOINT: &str = "/api/usage";
    const TICK: Duration = Duration::from_secs(1);
    const FLUSH_EVERY: Duration = Duration::from_secs(60);
    /// Media is sampled less often: finding the tab's domain runs osascript.
    const MEDIA_EVERY: Duration = Duration::from_secs(10);

    let observe = || Observation {
        now_ms: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64),
        app: mac::front_app(),
        idle_secs: mac::idle_seconds(),
        locked: mac::screen_locked(),
    };

    if let Some(i) = std::env::args().position(|a| a == "--dump-biome") {
        let args: Vec<String> = std::env::args().collect();
        let stream = args.get(i + 1).map_or("App.InFocus", String::as_str);
        screentime::dump_biome_raw(stream, args.get(i + 2).map(String::as_str));
        return ExitCode::SUCCESS;
    }
    if std::env::args().any(|a| a == "--dump-screen-time") {
        screentime::dump();
        return ExitCode::SUCCESS;
    }
    if std::env::args().any(|a| a == "--dump-now-playing") {
        match mac::now_playing() {
            Some(n) => println!("bundle={} playing={:?} title={:?} artist={:?}", n.bundle, n.playing, n.title, n.artist),
            None => println!("Now Playing: nothing (or macOS refused)"),
        }
        return ExitCode::SUCCESS;
    }
    if std::env::args().any(|a| a == "--dry-run") {
        mac::pump_run_loop(Duration::from_millis(100));
        let o = observe();
        println!(
            "device={} ({}) app={:?} idle={:.0}s locked={} accessibility={}\nplaying={:?}",
            mac::device_id(), mac::device_name(), o.app, o.idle_secs, o.locked, mac::accessibility_trusted(false), mac::playing()
        );
        return ExitCode::SUCCESS;
    }

    env_logger::Builder::new().filter_level(log::LevelFilter::Info).parse_default_env().init();
    if !mac::accessibility_trusted(true) {
        log::warn!("no Accessibility permission yet: window titles stay empty until it is granted");
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let queue = Queue::new(format!("{home}/Library/Application Support/usage-agent/queue.jsonl"));
    let prefs_path = format!("{home}/Library/Preferences/me.centralhardware.usage-agent.plist");

    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut tracker = Tracker::new(mac::device_id(), mac::device_name());
    let mut media = MediaTracker::new(mac::device_id(), mac::device_name());
    let mut last_media = Instant::now() - MEDIA_EVERY;
    let mut last_flush = Instant::now();
    let mut warned_no_url = false;
    loop {
        // Lets AppKit process workspace changes, so frontmostApplication stays current.
        mac::pump_run_loop(TICK);
        let o = observe();
        for b in tracker.tick(&o) {
            queue.push(&Item::Usage(b));
        }
        if last_media.elapsed() >= MEDIA_EVERY {
            last_media = Instant::now();
            let playing = if o.locked { None } else { mac::playing() };
            for m in media.tick(o.now_ms, playing.as_ref(), SLEEP_GAP_MS + MEDIA_EVERY.as_millis() as i64) {
                queue.push(&Item::Media(m));
            }
        }
        if last_flush.elapsed() < FLUSH_EVERY {
            continue;
        }
        last_flush = Instant::now();

        let prefs = read_prefs(&prefs_path);
        let Some(url) = std::env::var("USAGE_AGENT_URL").ok().or(prefs.0) else {
            if !warned_no_url {
                log::warn!("no ServerURL: defaults write me.centralhardware.usage-agent ServerURL https://host");
                warned_no_url = true;
            }
            continue;
        };
        warned_no_url = false;
        let url = if url.trim_end_matches('/').ends_with(ENDPOINT) { url } else { format!("{}{ENDPOINT}", url.trim_end_matches('/')) };
        let token = std::env::var("USAGE_AGENT_TOKEN").ok().or(prefs.1);
        let send = |body: &usage_model::UsageBatch| -> Result<(), SendError> {
            let mut req = agent.post(&url);
            if let Some(t) = &token {
                req = req.header("Authorization", &format!("Bearer {t}"));
            }
            match req.send_json(body) {
                Ok(r) if r.status().is_success() => Ok(()),
                Ok(r) => Err(SendError::from_status(r.status().as_u16(), format!("HTTP {}", r.status()))),
                Err(e) => Err(SendError::Retry(e.to_string())),
            }
        };
        let open: Vec<Item> = tracker.current().cloned().map(Item::Usage).into_iter().chain(media.current().cloned().map(Item::Media)).collect();
        if let Err(e) = queue.flush(open, send) {
            log::warn!("send failed, will retry: {e}");
        }
    }
}

/// (ServerURL, AuthToken) from the user's preferences file.
#[cfg(target_os = "macos")]
fn read_prefs(path: &str) -> (Option<String>, Option<String>) {
    let Ok(plist::Value::Dictionary(d)) = plist::Value::from_file(path) else { return (None, None) };
    let get = |k: &str| d.get(k).and_then(|v| v.as_string()).map(str::to_owned).filter(|s| !s.is_empty());
    (get("ServerURL"), get("AuthToken"))
}
