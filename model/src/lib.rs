//! One stretch of time on a Mac: an element of the `blocks` array of `POST /api/usage`
//! and a row of ClickHouse `mac_usage` (snake_case, field order = columns).

use serde::{Deserialize, Serialize};

pub const KIND_APP: &str = "app";
pub const KIND_AWAY: &str = "away";

/// Why nobody was at the Mac, in `bundle_id`/`app_name` of an away block.
pub const AWAY_IDLE: &str = "idle";
pub const AWAY_LOCK: &str = "lock";
pub const AWAY_SLEEP: &str = "sleep";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "clickhouse", derive(clickhouse::Row))]
pub struct UsageBlock {
    pub device_id: String,
    /// "app": `bundle_id` was frontmost; "away": nobody at the Mac, `bundle_id` says why
    /// (idle, lock, sleep).
    pub kind: String,
    pub bundle_id: String,
    pub app_name: String,
    /// The focused window's title; empty without the Accessibility permission.
    #[serde(default)]
    pub title: String,
    /// Unix milliseconds (ClickHouse `DateTime64(3)`).
    pub start_at: i64,
    /// Unix milliseconds. The agent re-sends the block it is still in with a later stop;
    /// the row with the latest stop per (device, start) wins.
    pub stop_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct UsageBatch {
    pub blocks: Vec<UsageBlock>,
}

impl UsageBlock {
    pub fn length_ms(&self) -> i64 {
        self.stop_at - self.start_at
    }

    /// Why the server refuses the block, if it does.
    pub fn problem(&self) -> Option<String> {
        for (name, v) in [("device_id", &self.device_id), ("bundle_id", &self.bundle_id), ("app_name", &self.app_name)] {
            if v.is_empty() || v.chars().count() > 256 {
                return Some(format!("{name} must be 1-256 characters"));
            }
        }
        if self.kind != KIND_APP && self.kind != KIND_AWAY {
            return Some("kind must be app or away".into());
        }
        if self.title.chars().count() > 1024 {
            return Some("title must be at most 1024 characters".into());
        }
        if self.stop_at < self.start_at {
            return Some("stop_at must not be before start_at".into());
        }
        if self.length_ms() > 24 * 3600 * 1000 {
            return Some("a block must be at most 24 hours".into());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> UsageBlock {
        UsageBlock {
            device_id: "mac".into(), kind: KIND_APP.into(), bundle_id: "com.apple.Safari".into(),
            app_name: "Safari".into(), title: String::new(), start_at: 1_000, stop_at: 2_000,
        }
    }

    #[test]
    fn validates() {
        assert_eq!(block().problem(), None);
        assert!(UsageBlock { kind: "x".into(), ..block() }.problem().is_some());
        assert!(UsageBlock { stop_at: 0, ..block() }.problem().is_some());
        assert!(UsageBlock { device_id: String::new(), ..block() }.problem().is_some());
        assert!(UsageBlock { stop_at: 1_000 + 25 * 3600 * 1000, ..block() }.problem().is_some());
    }
}
