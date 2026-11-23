//! Discovery for Screen Time data: what macOS stores locally, before we decide what to send.
//! Both stores need Full Disk Access for whatever process reads them.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Seconds between the Unix epoch and Apple's Core Data epoch (2001-01-01).
const APPLE_EPOCH: f64 = 978_307_200.0;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn fmt_ts(unix: f64) -> String {
    let secs = unix as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

pub fn dump() {
    dump_knowledge();
    println!();
    dump_biome();
}

fn dump_knowledge() {
    let path = home().join("Library/Application Support/Knowledge/knowledgeC.db");
    println!("== knowledgeC: {}", path.display());
    let db = match rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(db) => db,
        Err(e) => return println!("  cannot open: {e} (Full Disk Access?)"),
    };
    let streams = db.prepare(
        "SELECT ZSTREAMNAME, COUNT(*), MIN(ZSTARTDATE), MAX(ZENDDATE) FROM ZOBJECT GROUP BY ZSTREAMNAME ORDER BY 2 DESC",
    );
    match streams {
        Err(e) => println!("  query failed: {e} (Full Disk Access?)"),
        Ok(mut q) => {
            let rows = q.query_map([], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<f64>>(2)?, r.get::<_, Option<f64>>(3)?))
            });
            for (name, n, lo, hi) in rows.into_iter().flatten().flatten() {
                let span = match (lo, hi) {
                    (Some(lo), Some(hi)) => format!("{} .. {}", fmt_ts(lo + APPLE_EPOCH), fmt_ts(hi + APPLE_EPOCH)),
                    _ => String::new(),
                };
                println!("  {:<40} {n:>7}  {span}", name.unwrap_or_default());
            }
        }
    }
    let devices = db.prepare(
        "SELECT IFNULL(s.ZDEVICEID, '(this mac)'), COUNT(*), MAX(o.ZENDDATE) FROM ZOBJECT o \
         LEFT JOIN ZSOURCE s ON o.ZSOURCE = s.Z_PK WHERE o.ZSTREAMNAME = '/app/usage' GROUP BY 1",
    );
    if let Ok(mut q) = devices {
        println!("  -- /app/usage by device");
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<f64>>(2)?)));
        for (dev, n, hi) in rows.into_iter().flatten().flatten() {
            println!("  {dev:<40} {n:>7}  last {}", hi.map(|h| fmt_ts(h + APPLE_EPOCH)).unwrap_or_default());
        }
    }
    let latest = db.prepare(
        "SELECT ZSTREAMNAME, ZVALUESTRING, ZSTARTDATE, ZENDDATE FROM ZOBJECT \
         WHERE ZSTREAMNAME IN ('/app/usage', '/app/webUsage', '/device/isLocked') ORDER BY ZENDDATE DESC LIMIT 10",
    );
    if let Ok(mut q) = latest {
        println!("  -- latest events");
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?))
        });
        for (s, v, a, b) in rows.into_iter().flatten().flatten() {
            println!("  {} {s:<18} {:>6.0}s  {}", fmt_ts(a + APPLE_EPOCH), b - a, v.unwrap_or_default());
        }
    }
}

fn dump_biome() {
    let root = home().join("Library/Biome/streams");
    println!("== Biome: {}", root.display());
    for scope in ["public", "restricted"] {
        let Ok(dir) = std::fs::read_dir(root.join(scope)) else {
            println!("  {scope}: cannot read (Full Disk Access?)");
            continue;
        };
        let mut names: Vec<_> = dir.flatten().map(|e| e.path()).collect();
        names.sort();
        for stream in names {
            let name = stream.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let interesting = ["App", "Device", "Screen", "Safari", "Web", "Notification", "Lock", "Backlight"]
                .iter()
                .any(|k| name.contains(k));
            if !interesting {
                continue;
            }
            let (files, bytes, newest) = scan(&stream.join("local"));
            println!("  {scope}/{name:<36} local: {files} files {bytes} B newest {newest}");
            if let Ok(remote) = std::fs::read_dir(stream.join("remote")) {
                for dev in remote.flatten() {
                    let (f, b, n) = scan(&dev.path());
                    println!("      remote {:<38} {f} files {b} B newest {n}", dev.file_name().to_string_lossy());
                }
            }
        }
    }
}

/// File count, total size and newest mtime under a directory (recursive).
fn scan(dir: &Path) -> (usize, u64, String) {
    let (mut files, mut bytes, mut newest) = (0, 0, SystemTime::UNIX_EPOCH);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                stack.push(e.path());
            } else {
                files += 1;
                bytes += md.len();
                newest = newest.max(md.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            }
        }
    }
    let newest = newest.duration_since(SystemTime::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    (files, bytes, if newest > 0.0 { fmt_ts(newest) } else { "-".into() })
}

/// `--dump-biome <Stream> [remote-device-id]`: hex of the newest SEGB file's head and tail, to learn the format.
pub fn dump_biome_raw(stream: &str, device: Option<&str>) {
    let base = home().join("Library/Biome/streams/restricted").join(stream);
    let dir = match device {
        Some(d) => base.join("remote").join(d),
        None => base.join("local"),
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let Some(path) = files.last() else { return println!("no files in {}", dir.display()) };
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return println!("{}: {e}", path.display()),
    };
    let used = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    println!("{} size={} used={}", path.display(), data.len(), used);
    println!("-- head");
    hex(&data[..data.len().min(768)], 0);
    let tail = used.saturating_sub(512);
    println!("-- tail (last non-zero bytes)");
    hex(&data[tail..used], tail);
}

fn hex(b: &[u8], base: usize) {
    for (i, row) in b.chunks(16).enumerate() {
        let h: Vec<_> = row.iter().map(|x| format!("{x:02x}")).collect();
        let a: String = row.iter().map(|&x| if (0x20..0x7f).contains(&x) { x as char } else { '.' }).collect();
        println!("{:06x}  {:<47}  {a}", base + i * 16, h.join(" "));
    }
}

/// Reads new Screen Time stretches since the cursors saved at `state_path`:
/// apps in focus on other synced devices (Biome `App.InFocus/remote/*`; this Mac is already
/// tracked live) and per-site Safari time from knowledgeC. The first run backfills everything.
/// Re-sending is harmless (`screen_time` is a ReplacingMergeTree), so cursors may lag.
pub fn import(device_id: &str, state_path: &Path) -> Vec<usage_model::ScreenTimeBlock> {
    use std::collections::BTreeMap;
    use usage_model::ScreenTimeBlock;
    let mut cursors: BTreeMap<String, i64> =
        std::fs::read(state_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut out = Vec::new();

    let remote = home().join("Library/Biome/streams/restricted/App.InFocus/remote");
    for dev in std::fs::read_dir(&remote).into_iter().flatten().flatten() {
        let id = dev.file_name().to_string_lossy().into_owned();
        let key = format!("biome:{id}");
        let since = cursors.get(&key).copied().unwrap_or(0);
        let mut events = Vec::new();
        for f in std::fs::read_dir(dev.path()).into_iter().flatten().flatten() {
            let fresh = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .is_none_or(|d| d.as_millis() as i64 >= since);
            if fresh && let Ok(data) = std::fs::read(f.path()) {
                events.extend(usage_agent::biome::parse_segb(&data));
            }
        }
        let mut max = since;
        for (bundle_id, start_at, stop_at) in usage_agent::biome::blocks(events) {
            if stop_at <= since {
                continue;
            }
            max = max.max(stop_at);
            out.push(ScreenTimeBlock {
                device_id: device_id.into(), source_device: id.clone(), kind: "app".into(), bundle_id, domain: String::new(), start_at, stop_at,
            });
        }
        cursors.insert(key, max);
    }

    let since = cursors.get("knowledge:web").copied().unwrap_or(0);
    match web_usage(since) {
        Ok(rows) => {
            let mut max = since;
            for (bundle_id, domain, start_at, stop_at) in rows {
                max = max.max(stop_at);
                out.push(ScreenTimeBlock {
                    device_id: device_id.into(), source_device: device_id.into(), kind: "web".into(), bundle_id, domain, start_at, stop_at,
                });
            }
            cursors.insert("knowledge:web".into(), max);
        }
        Err(e) => {
            log::warn!("knowledgeC web usage: {e} (Full Disk Access?)");
            eprintln!("knowledgeC web usage: {e}");
        }
    }

    save_cursors(state_path, &cursors);
    out
}

/// knowledgeC streams kept as generic events.
pub const EVENT_STREAMS: [&str; 5] = ["/notification/usage", "/display/isBacklit", "/app/mediaUsage", "/app/intents", "/bluetooth/isConnected"];

/// New knowledgeC events (`EVENT_STREAMS`) since the cursor in `state_path`; first run backfills.
pub fn import_events(device_id: &str, state_path: &Path) -> Vec<usage_model::EventBlock> {
    use std::collections::BTreeMap;
    let mut cursors: BTreeMap<String, i64> =
        std::fs::read(state_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let since = cursors.get("knowledge:events").copied().unwrap_or(0);
    let out = match knowledge_events(device_id, since) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("knowledgeC events: {e} (Full Disk Access?)");
            eprintln!("knowledgeC events: {e}");
            return vec![];
        }
    };
    if let Some(max) = out.iter().map(|e| e.stop_at).max() {
        cursors.insert("knowledge:events".into(), max);
    }
    save_cursors(state_path, &cursors);
    out
}

fn save_cursors(state_path: &Path, cursors: &std::collections::BTreeMap<String, i64>) {
    if let Some(dir) = state_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(state_path, serde_json::to_vec(cursors).unwrap_or_default());
}

/// "Z_DKINTENTMETADATAKEY__INTENTCLASS" -> "intent.intentclass".
fn field_name(col: &str) -> String {
    let c = col.strip_prefix("Z_DK").or_else(|| col.strip_prefix('Z')).unwrap_or(col);
    c.replacen("METADATAKEY__", ".", 1).to_lowercase()
}

fn looks_like_bundle(s: &str) -> bool {
    s.contains('.') && !s.contains(' ') && !s.contains('/')
}

fn knowledge_events(device_id: &str, since_ms: i64) -> Result<Vec<usage_model::EventBlock>, rusqlite::Error> {
    use rusqlite::types::ValueRef;
    let path = home().join("Library/Application Support/Knowledge/knowledgeC.db");
    let db = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let cols = |t: &str| -> Result<usize, rusqlite::Error> {
        let mut q = db.prepare(&format!("PRAGMA table_info({t})"))?;
        let n = q.query_map([], |_| Ok(()))?.count();
        Ok(n)
    };
    let (n_obj, n_src) = (cols("ZOBJECT")?, cols("ZSOURCE")?);
    let streams = EVENT_STREAMS.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",");
    let mut q = db.prepare(&format!(
        "SELECT o.*, s.*, m.* FROM ZOBJECT o LEFT JOIN ZSOURCE s ON o.ZSOURCE = s.Z_PK \
         LEFT JOIN ZSTRUCTUREDMETADATA m ON o.ZSTRUCTUREDMETADATA = m.Z_PK \
         WHERE o.ZSTREAMNAME IN ({streams}) AND o.ZENDDATE > ?1 ORDER BY o.ZENDDATE"
    ))?;
    let names: Vec<String> = q.column_names().into_iter().map(str::to_owned).collect();
    let since = since_ms as f64 / 1000.0 - APPLE_EPOCH;
    let text = |v: ValueRef| -> Option<String> {
        match v {
            ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()).filter(|s| !s.is_empty()),
            ValueRef::Integer(i) => Some(i.to_string()),
            ValueRef::Real(f) => Some(f.to_string()),
            _ => None,
        }
    };
    let mut rows = q.query([since])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let mut obj = std::collections::HashMap::new();
        let mut src = std::collections::HashMap::new();
        let mut fields: Vec<(String, String)> = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let Some(v) = text(r.get_ref(i)?) else { continue };
            if i < n_obj {
                obj.insert(name.as_str(), v);
            } else if i < n_obj + n_src {
                src.insert(name.as_str(), v);
            } else if !matches!(name.as_str(), "Z_PK" | "Z_ENT" | "Z_OPT") && fields.len() < 64 {
                fields.push((field_name(name), v.chars().take(4096).collect()));
            }
        }
        let num = |k: &str| obj.get(k).and_then(|v| v.parse::<f64>().ok());
        let (Some(stream), Some(start), Some(end)) = (obj.get("ZSTREAMNAME").cloned(), num("ZSTARTDATE"), num("ZENDDATE")) else { continue };
        let vs = obj.get("ZVALUESTRING").cloned().unwrap_or_default();
        let vi = obj.get("ZVALUEINTEGER").cloned().unwrap_or_default();
        let src_bundle = src.get("ZBUNDLEID").cloned().unwrap_or_default();
        // Some streams keep the app only in metadata (notificationusage.bundleid).
        let meta_bundle = fields.iter().find(|(k, v)| k.ends_with(".bundleid") && looks_like_bundle(v)).map(|(_, v)| v.to_owned());
        let (bundle_id, value) = if let Some(b) = meta_bundle.filter(|_| src_bundle.is_empty() && !looks_like_bundle(&vs)) {
            (b, if vs.is_empty() { vi } else { vs })
        } else if !src_bundle.is_empty() {
            (src_bundle, if vs.is_empty() { vi } else { vs })
        } else if looks_like_bundle(&vs) {
            (vs, vi)
        } else {
            (String::new(), if vs.is_empty() { vi } else { vs })
        };
        let ms = |x: f64| ((x + APPLE_EPOCH) * 1000.0) as i64;
        let (start_at, stop_at) = (ms(start), ms(end.max(start)));
        if stop_at - start_at > 7 * 24 * 3600 * 1000 {
            continue;
        }
        out.push(usage_model::EventBlock {
            device_id: device_id.into(),
            source_device: src.get("ZDEVICEID").cloned().unwrap_or_else(|| device_id.into()),
            stream,
            bundle_id: bundle_id.chars().take(256).collect(),
            value: value.chars().take(1024).collect(),
            fields,
            start_at,
            stop_at,
        });
    }
    Ok(out)
}

/// (bundle id, domain, start ms, stop ms) of knowledgeC `/app/webUsage` ending after `since_ms`.
fn web_usage(since_ms: i64) -> Result<Vec<(String, String, i64, i64)>, rusqlite::Error> {
    let path = home().join("Library/Application Support/Knowledge/knowledgeC.db");
    let db = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut q = db.prepare(
        "SELECT o.ZVALUESTRING, IFNULL(m.Z_DKDIGITALHEALTHMETADATAKEY__WEBDOMAIN, ''), o.ZSTARTDATE, o.ZENDDATE \
         FROM ZOBJECT o LEFT JOIN ZSTRUCTUREDMETADATA m ON o.ZSTRUCTUREDMETADATA = m.Z_PK \
         WHERE o.ZSTREAMNAME = '/app/webUsage' AND o.ZENDDATE > ?1 AND o.ZENDDATE > o.ZSTARTDATE ORDER BY o.ZENDDATE",
    )?;
    let since = since_ms as f64 / 1000.0 - APPLE_EPOCH;
    let rows = q.query_map([since], |r| {
        let ms = |x: f64| ((x + APPLE_EPOCH) * 1000.0) as i64;
        Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get::<_, String>(1)?, ms(r.get(2)?), ms(r.get(3)?)))
    })?;
    Ok(rows.flatten().filter(|(b, _, s, e)| !b.is_empty() && e - s <= 24 * 3600 * 1000).collect())
}

/// `--dump-screen-time-import`: what `import` would send, without saving cursors.
pub fn dump_import(device_id: &str) {
    let tmp = std::env::temp_dir().join("usage-agent-screentime-dry.json");
    let _ = std::fs::remove_file(&tmp);
    let blocks = import(device_id, &tmp);
    let mut events = import_events(device_id, &tmp);
    events.extend(import_biome_events(device_id, &tmp));
    let _ = std::fs::remove_file(&tmp);
    let mut per_stream: std::collections::BTreeMap<&str, usize> = Default::default();
    for e in &events {
        *per_stream.entry(e.stream.as_str()).or_default() += 1;
    }
    for (s, n) in per_stream {
        println!("event {s:<32} {n:>6}");
    }
    for e in events.iter().rev().take(10) {
        println!("  {} {} {} {:?} {:?}", fmt_ts(e.start_at as f64 / 1000.0), e.stream, e.bundle_id, e.value, e.fields);
    }
    let mut per: std::collections::BTreeMap<(String, String), (usize, i64)> = Default::default();
    for b in &blocks {
        let e = per.entry((b.source_device.clone(), b.kind.clone())).or_default();
        e.0 += 1;
        e.1 = e.1.max(b.stop_at);
    }
    for ((dev, kind), (n, last)) in per {
        println!("{dev:<38} {kind:<4} {n:>6} blocks, last {}", fmt_ts(last as f64 / 1000.0));
    }
    for b in blocks.iter().rev().take(15) {
        println!("  {} {:>5}s {} {} {}", fmt_ts(b.start_at as f64 / 1000.0), (b.stop_at - b.start_at) / 1000, &b.source_device[..8.min(b.source_device.len())], b.bundle_id, b.domain);
    }
}

/// Biome streams kept as generic events (App.InFocus has its own reader → screen_time).
/// Left out: ProactiveHarvesting.Safari.PageView and App.Intents.Transcript (page/conversation text).
pub const BIOME_STREAMS: [&str; 12] = [
    "App.WebUsage", "App.MediaUsage", "App.WebApp.InFocus", "App.MenuItem", "App.DocumentInteraction", "ScreenTime.AppUsage",
    "Notification.Usage", "Notification.Delivery", "Safari.Navigations", "Screenshots.Screenshot", "Device.Wireless.Bluetooth",
    "App.Intent",
];
/// Most events one run queues, so a backfill can't push the queue past its cap; the rest comes next run.
const BIOME_BUDGET: usize = 20_000;

pub fn import_biome_events(device_id: &str, state_path: &Path) -> Vec<usage_model::EventBlock> {
    use std::collections::BTreeMap;
    let mut cursors: BTreeMap<String, i64> =
        std::fs::read(state_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let root = home().join("Library/Biome/streams/restricted");
    let mut out = Vec::new();
    for stream in BIOME_STREAMS {
        let base = root.join(stream);
        let mut dirs = vec![(device_id.to_owned(), base.join("local"))];
        for d in std::fs::read_dir(base.join("remote")).into_iter().flatten().flatten() {
            dirs.push((d.file_name().to_string_lossy().into_owned(), d.path()));
        }
        for (dev, dir) in dirs {
            if out.len() >= BIOME_BUDGET {
                break;
            }
            let key = format!("biome:{stream}:{dev}");
            let since = cursors.get(&key).copied().unwrap_or(0);
            let mut evs = Vec::new();
            for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let fresh = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .is_none_or(|d| d.as_millis() as i64 >= since);
                let Ok(data) = (if fresh { std::fs::read(f.path()) } else { continue }) else { continue };
                for (at, payload) in usage_agent::biome::records(&data) {
                    if at <= since {
                        continue;
                    }
                    let Some(mut fields) = usage_agent::biome::decode(payload) else { continue };
                    fields.truncate(64);
                    let bundle_id = pick_bundle(&fields);
                    evs.push(usage_model::EventBlock {
                        device_id: device_id.into(), source_device: dev.clone(), stream: format!("biome:{stream}"),
                        bundle_id, value: String::new(), fields, start_at: at, stop_at: at,
                    });
                }
            }
            evs.sort_by_key(|e| e.start_at);
            evs.truncate(BIOME_BUDGET - out.len());
            if let Some(last) = evs.last() {
                cursors.insert(key, last.start_at);
            }
            out.extend(evs);
        }
    }
    save_cursors(state_path, &cursors);
    out
}

/// `--dump-biome-decoded <Stream> [device]`: the newest records of a stream, decoded generically.
pub fn dump_biome_decoded(stream: &str, device: Option<&str>) {
    let base = home().join("Library/Biome/streams/restricted").join(stream);
    let dir = device.map_or(base.join("local"), |d| base.join("remote").join(d));
    let mut recs = Vec::new();
    for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if let Ok(data) = std::fs::read(f.path()) {
            for (at, p) in usage_agent::biome::records(&data) {
                recs.push((at, usage_agent::biome::decode(p)));
            }
        }
    }
    recs.sort_by_key(|r| r.0);
    println!("{}: {} records", dir.display(), recs.len());
    for (at, d) in recs.iter().rev().take(8) {
        println!("{} {:?}", fmt_ts(*at as f64 / 1000.0), d);
    }
}

/// The record's app: a bundle-looking value that is not a website (a URL's host in the same record).
fn pick_bundle(fields: &[(String, String)]) -> String {
    let hosts: Vec<&str> = fields
        .iter()
        .filter_map(|(_, v)| v.strip_prefix("https://").or_else(|| v.strip_prefix("http://")))
        .map(|rest| rest.split(['/', '?', '#', ':']).next().unwrap_or(""))
        .collect();
    fields
        .iter()
        .map(|(_, v)| v)
        .find(|v| looks_like_bundle(v) && v.len() <= 256 && !v.starts_with("www.") && !hosts.iter().any(|h| *h == v.as_str() || h.ends_with(&format!(".{v}"))))
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundle_is_not_the_website() {
        let f = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let rec = f(&[("4", "https://www.youtube.com/shorts/x"), ("5", "youtube.com"), ("6", "com.apple.Safari")]);
        assert_eq!(super::pick_bundle(&rec), "com.apple.Safari");
        assert_eq!(super::pick_bundle(&f(&[("5", "www.example.org"), ("6", "app.swiftgram.ios")])), "app.swiftgram.ios");
    }
}
