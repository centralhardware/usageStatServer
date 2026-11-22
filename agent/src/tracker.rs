//! The state machine behind the agent, kept free of macOS calls so it can be tested anywhere.
//! Each tick it gets what the Mac looks like right now and returns the blocks that just closed.

use usage_model::{AWAY_IDLE, AWAY_LOCK, AWAY_SLEEP, KIND_APP, KIND_AWAY, UsageBlock};

/// No input for this long counts as away; the away block starts at the last input.
pub const IDLE_LIMIT_SECS: f64 = 300.0;
/// App blocks shorter than this (alt-tabbing through) are dropped.
pub const MIN_APP_MS: i64 = 2_000;
/// A title change this soon after the block began renames it instead of splitting it
/// (a page still loading, a terminal redrawing its title).
pub const TITLE_SETTLE_MS: i64 = 5_000;
/// Long blocks are cut so an upload stays small and a crash loses little.
pub const MAX_BLOCK_MS: i64 = 3_600_000;
/// A gap this long between two ticks means the Mac was asleep.
pub const SLEEP_GAP_MS: i64 = 30_000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontApp {
    pub bundle_id: String,
    pub name: String,
    pub title: String,
}

/// What the Mac looks like at one tick.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    pub now_ms: i64,
    pub app: Option<FrontApp>,
    pub idle_secs: f64,
    pub locked: bool,
}

pub struct Tracker {
    device: String,
    current: Option<UsageBlock>,
    last_tick: Option<i64>,
}

impl Tracker {
    pub fn new(device: impl Into<String>) -> Self {
        Self { device: device.into(), current: None, last_tick: None }
    }

    /// The open block, with its stop moved to the last tick; re-sent on every flush.
    pub fn current(&self) -> Option<&UsageBlock> {
        self.current.as_ref().filter(|b| b.kind == KIND_AWAY || b.length_ms() >= MIN_APP_MS)
    }

    fn block(&self, kind: &str, bundle: &str, name: &str, title: &str, at: i64) -> UsageBlock {
        UsageBlock {
            device_id: self.device.clone(), kind: kind.into(), bundle_id: bundle.into(), app_name: name.into(),
            title: title.into(), start_at: at, stop_at: at,
        }
    }

    fn close(&mut self, at: i64, out: &mut Vec<UsageBlock>) {
        if let Some(mut b) = self.current.take() {
            b.stop_at = at.max(b.start_at);
            if b.kind == KIND_AWAY || b.length_ms() >= MIN_APP_MS {
                out.push(b);
            }
        }
    }

    fn away_reason(&self) -> Option<&str> {
        self.current.as_ref().filter(|b| b.kind == KIND_AWAY).map(|b| b.bundle_id.as_str())
    }

    /// Starts an away block, unless one is open already. An idle stretch that turns into a lock
    /// or sleep becomes a block of its own, so the pickup is counted; a lock followed by sleep
    /// (or the reverse) stays one block.
    fn go_away(&mut self, reason: &str, at: i64, out: &mut Vec<UsageBlock>) {
        match self.away_reason() {
            Some(r) if r != AWAY_IDLE || reason == AWAY_IDLE => return,
            _ => {}
        }
        self.close(at, out);
        self.current = Some(self.block(KIND_AWAY, reason, reason, "", at));
    }

    pub fn tick(&mut self, o: &Observation) -> Vec<UsageBlock> {
        let mut out = Vec::new();
        let now = o.now_ms;

        // Asleep since the last tick: the open block ends there and the gap is an away block.
        if let Some(last) = self.last_tick
            && now - last >= SLEEP_GAP_MS
        {
            self.go_away(AWAY_SLEEP, last, &mut out);
        }
        self.last_tick = Some(now);

        if o.locked {
            self.go_away(AWAY_LOCK, now, &mut out);
        } else if o.idle_secs >= IDLE_LIMIT_SECS {
            let since = now - (o.idle_secs * 1000.0) as i64;
            if self.away_reason().is_none() {
                self.go_away(AWAY_IDLE, since.max(self.current.as_ref().map_or(since, |b| b.start_at)), &mut out);
            }
        } else if self.away_reason().is_some() {
            // Back at the Mac.
            self.close(now, &mut out);
        }

        if self.away_reason().is_none() {
            match (&o.app, &self.current) {
                (None, _) => self.close(now, &mut out),
                (Some(a), Some(b)) if a.bundle_id == b.bundle_id && a.title == b.title => {}
                (Some(a), Some(b)) if a.bundle_id == b.bundle_id && now - b.start_at < TITLE_SETTLE_MS => {
                    if let Some(b) = self.current.as_mut() {
                        b.title = a.title.clone();
                    }
                }
                (Some(a), _) => {
                    self.close(now, &mut out);
                    self.current = Some(self.block(KIND_APP, &a.bundle_id, &a.name, &a.title, now));
                }
            }
        }

        if let Some(b) = self.current.as_mut() {
            b.stop_at = now.max(b.start_at);
            if b.length_ms() >= MAX_BLOCK_MS {
                out.push(b.clone());
                b.start_at = now;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, title: &str) -> Option<FrontApp> {
        Some(FrontApp { bundle_id: id.into(), name: id.into(), title: title.into() })
    }

    /// Ticks every 2 s over [from, to] seconds with the same state; idle grows with time
    /// when `idle_from` is set.
    fn run(t: &mut Tracker, from: i64, to: i64, a: Option<FrontApp>, idle_from: Option<i64>, locked: bool) -> Vec<UsageBlock> {
        let mut out = Vec::new();
        let mut s = from;
        while s <= to {
            let idle_secs = idle_from.map_or(0.0, |i| (s - i) as f64);
            out.extend(t.tick(&Observation { now_ms: s * 1000, app: a.clone(), idle_secs, locked }));
            s += 2;
        }
        out
    }

    #[test]
    fn switching_apps_closes_blocks() {
        let mut t = Tracker::new("mac");
        assert!(run(&mut t, 0, 10, app("a", "x"), None, false).is_empty());
        let out = run(&mut t, 12, 20, app("b", "y"), None, false);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].bundle_id.as_str(), out[0].start_at, out[0].stop_at), ("a", 0, 12_000));
        assert_eq!(t.current().unwrap().bundle_id, "b");
    }

    #[test]
    fn short_blocks_are_dropped_and_early_titles_rename() {
        let mut t = Tracker::new("mac");
        run(&mut t, 0, 0, app("a", ""), None, false);
        assert!(run(&mut t, 1, 1, app("b", ""), None, false).is_empty()); // a lasted 1 s
        run(&mut t, 3, 3, app("b", "loaded"), None, false); // within settle: renamed
        run(&mut t, 5, 19, app("b", "loaded"), None, false);
        let out = run(&mut t, 21, 21, app("b", "other page"), None, false);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].title.as_str(), out[0].start_at), ("loaded", 1_000));
    }

    #[test]
    fn idle_is_backdated_and_ends_on_input() {
        let mut t = Tracker::new("mac");
        run(&mut t, 0, 50, app("a", ""), None, false);
        // No input from 50 s on: away once 5 min have passed, starting at 50 s.
        let out = run(&mut t, 52, 400, app("a", ""), Some(50), false);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].bundle_id.as_str(), out[0].stop_at), ("a", 50_000));
        assert_eq!(t.current().unwrap().bundle_id, AWAY_IDLE);
        let out = run(&mut t, 402, 402, app("a", ""), None, false);
        assert_eq!((out[0].kind.as_str(), out[0].start_at, out[0].stop_at), (KIND_AWAY, 50_000, 402_000));
    }

    #[test]
    fn idle_then_lock_splits_and_sleep_gap_is_away() {
        let mut t = Tracker::new("mac");
        run(&mut t, 0, 50, app("a", ""), None, false);
        run(&mut t, 52, 400, app("a", ""), Some(50), false);
        let out = run(&mut t, 402, 402, app("a", ""), Some(50), true);
        assert_eq!(out.last().unwrap().bundle_id, AWAY_IDLE);
        assert_eq!(t.current().unwrap().bundle_id, AWAY_LOCK);
        // Asleep while locked (a gap in the ticks) stays one lock block.
        assert!(run(&mut t, 1000, 1000, app("a", ""), None, true).is_empty());
        let out = run(&mut t, 1002, 1002, app("a", ""), None, false);
        assert_eq!((out[0].bundle_id.as_str(), out[0].start_at), (AWAY_LOCK, 402_000));

        // An unlocked sleep: the gap is a sleep block, then the app again.
        run(&mut t, 1004, 1010, app("a", ""), None, false);
        let out = run(&mut t, 2000, 2000, app("a", ""), None, false);
        let a = out.iter().find(|b| b.kind == KIND_APP).unwrap();
        assert_eq!((a.start_at, a.stop_at), (1_002_000, 1_010_000));
        let sleep = out.iter().find(|b| b.bundle_id == AWAY_SLEEP).unwrap();
        assert_eq!((sleep.start_at, sleep.stop_at), (1_010_000, 2_000_000));
    }

    #[test]
    fn long_blocks_are_cut() {
        let mut t = Tracker::new("mac");
        let out = run(&mut t, 0, 3700, app("a", ""), None, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].length_ms(), MAX_BLOCK_MS);
    }
}
