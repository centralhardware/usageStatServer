//! One stretch of time on a Mac: an element of the `blocks` array of `POST /api/usage`
//! and a row of ClickHouse `mac_usage` (snake_case, field order = columns).

use serde::{Deserialize, Serialize};

pub const KIND_APP: &str = "app";
pub const KIND_AWAY: &str = "away";

/// Why nobody was at the Mac, in `bundle_id`/`app_name` of an away block.
pub const AWAY_IDLE: &str = "idle";
pub const AWAY_LOCK: &str = "lock";
pub const AWAY_SLEEP: &str = "sleep";
/// Another macOS account is on the console (fast user switching): this one is in the background.
pub const AWAY_SWITCHED: &str = "switched";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "clickhouse", derive(clickhouse::Row))]
pub struct UsageBlock {
    /// The Mac's IOPlatformUUID: stable across renames, unique per machine.
    pub device_id: String,
    /// The Mac's name (System Settings → General → Sharing), for display.
    #[serde(default)]
    pub device_name: String,
    /// The macOS account the agent runs in (short name); several accounts can share one Mac.
    #[serde(default)]
    pub user_name: String,
    /// "app": `bundle_id` was frontmost; "away": nobody at the Mac, `bundle_id` says why
    /// (idle, lock, sleep).
    pub kind: String,
    pub bundle_id: String,
    pub app_name: String,
    /// The focused window's title; empty without the Accessibility permission.
    #[serde(default)]
    pub title: String,
    /// Parsed from the title by the server (title_rules): the thing you're on and where.
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub context: String,
    /// The matching rule's other named groups (ClickHouse `Map`), e.g. ("host", "10.168.0.77").
    #[serde(default)]
    pub fields: Vec<(String, String)>,
    /// The front tab's domain when the app is a browser (youtube.com), else empty.
    #[serde(default)]
    pub domain: String,
    /// The app's declared category (LSApplicationCategoryType), e.g.
    /// public.app-category.developer-tools; empty when it declares none.
    #[serde(default)]
    pub app_category: String,
    /// Unix milliseconds (ClickHouse `DateTime64(3)`).
    pub start_at: i64,
    /// Unix milliseconds. The agent re-sends the block it is still in with a later stop;
    /// the row with the latest stop per (device, start) wins.
    pub stop_at: i64,
}

/// A stretch of video playing: a row of ClickHouse `mac_media`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "clickhouse", derive(clickhouse::Row))]
pub struct MediaBlock {
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    /// The macOS account the agent runs in (short name); several accounts can share one Mac.
    #[serde(default)]
    pub user_name: String,
    /// Where the video comes from: the browser tab's domain (youtube.com), else the app's name.
    pub source: String,
    /// The app playing it.
    pub bundle_id: String,
    pub app_name: String,
    /// From macOS Now Playing: the media's title and artist/channel; empty when unknown.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    /// Unix milliseconds.
    pub start_at: i64,
    pub stop_at: i64,
}

impl MediaBlock {
    pub fn problem(&self) -> Option<String> {
        for (name, v) in [("device_id", &self.device_id), ("source", &self.source), ("bundle_id", &self.bundle_id), ("app_name", &self.app_name)] {
            if v.is_empty() || v.chars().count() > 256 {
                return Some(format!("media {name} must be 1-256 characters"));
            }
        }
        if self.title.chars().count() > 1024 || self.artist.chars().count() > 1024 {
            return Some("media title and artist must be at most 1024 characters".into());
        }
        if self.stop_at < self.start_at || self.stop_at - self.start_at > 24 * 3600 * 1000 {
            return Some("media block must run forward and last at most 24 hours".into());
        }
        None
    }
}

/// A stretch from macOS Screen Time stores, read by the agent: an app in focus on any synced
/// device (Biome App.InFocus, incl. iPhone/iPad), or time on a website (knowledgeC webUsage).
/// A row of ClickHouse `screen_time`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "clickhouse", derive(clickhouse::Row))]
pub struct ScreenTimeBlock {
    /// The Mac that read it.
    pub device_id: String,
    /// The device it happened on: a Biome remote device id, or `device_id` for this Mac.
    pub source_device: String,
    /// "app" or "web".
    pub kind: String,
    pub bundle_id: String,
    /// The website for kind "web", else empty.
    #[serde(default)]
    pub domain: String,
    pub start_at: i64,
    pub stop_at: i64,
}

impl ScreenTimeBlock {
    pub fn problem(&self) -> Option<String> {
        for (name, v) in [("device_id", &self.device_id), ("source_device", &self.source_device), ("bundle_id", &self.bundle_id)] {
            if v.is_empty() || v.chars().count() > 256 {
                return Some(format!("screen_time {name} must be 1-256 characters"));
            }
        }
        if self.kind != "app" && self.kind != "web" {
            return Some("screen_time kind must be app or web".into());
        }
        if self.domain.chars().count() > 1024 {
            return Some("screen_time domain must be at most 1024 characters".into());
        }
        if self.stop_at < self.start_at || self.stop_at - self.start_at > 24 * 3600 * 1000 {
            return Some("screen_time block must run forward and last at most 24 hours".into());
        }
        None
    }
}

/// One event from another macOS store (knowledgeC streams: notifications, screen on/off, media,
/// app intents, Bluetooth). A row of ClickHouse `events`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "clickhouse", derive(clickhouse::Row))]
pub struct EventBlock {
    /// The Mac that read it.
    pub device_id: String,
    /// The device it happened on (a synced device's id, or `device_id`).
    pub source_device: String,
    /// The store's stream, e.g. "/notification/usage".
    pub stream: String,
    /// The app it concerns; empty when none.
    #[serde(default)]
    pub bundle_id: String,
    /// The stream's own value: notification action, 1/0 for on/off states, a device name.
    #[serde(default)]
    pub value: String,
    /// Every other non-empty attribute the store keeps for it (ClickHouse `Map`).
    #[serde(default)]
    pub fields: Vec<(String, String)>,
    pub start_at: i64,
    pub stop_at: i64,
}

impl EventBlock {
    pub fn problem(&self) -> Option<String> {
        for (name, v) in [("device_id", &self.device_id), ("source_device", &self.source_device), ("stream", &self.stream)] {
            if v.is_empty() || v.chars().count() > 256 {
                return Some(format!("event {name} must be 1-256 characters"));
            }
        }
        if self.bundle_id.chars().count() > 256 || self.value.chars().count() > 1024 {
            return Some("event bundle_id/value too long".into());
        }
        if self.fields.len() > 64 || self.fields.iter().any(|(k, v)| k.chars().count() > 256 || v.chars().count() > 4096) {
            return Some("event fields: at most 64, keys 256 and values 4096 characters".into());
        }
        if self.stop_at < self.start_at || self.stop_at - self.start_at > 7 * 24 * 3600 * 1000 {
            return Some("event must run forward and last at most 7 days".into());
        }
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct UsageBatch {
    #[serde(default)]
    pub blocks: Vec<UsageBlock>,
    #[serde(default)]
    pub media: Vec<MediaBlock>,
    #[serde(default)]
    pub screen_time: Vec<ScreenTimeBlock>,
    #[serde(default)]
    pub events: Vec<EventBlock>,
}

impl UsageBatch {
    pub fn len(&self) -> usize {
        self.blocks.len() + self.media.len() + self.screen_time.len() + self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
            device_id: "mac".into(), device_name: "Mac".into(), kind: KIND_APP.into(), bundle_id: "com.apple.Safari".into(),
            app_name: "Safari".into(), title: String::new(), domain: String::new(), app_category: String::new(), start_at: 1_000, stop_at: 2_000, ..Default::default()
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
