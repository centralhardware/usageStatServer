//! Closed blocks not yet delivered (offline, server down), one JSON per
//! line. Sent oldest first, in batches, once the server answers again.

use serde::{Deserialize, Serialize};
use usage_model::{MediaBlock, UsageBatch, UsageBlock};

/// One queued line. Untagged, so lines written before media existed (plain blocks) still read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Item {
    Usage(UsageBlock),
    Media(MediaBlock),
}
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Weeks of blocks; the oldest are dropped beyond it.
pub const MAX_LINES: usize = 100_000;
/// Blocks per request.
pub const FLUSH_BATCH: usize = 500;
/// Past this size `push` trims the file back to `MAX_LINES`, so the queue stays
/// bounded even when nothing ever flushes it (no ServerURL set).
const TRIM_BYTES: u64 = 200 * 1024 * 1024;

/// Why a send failed.
#[derive(Debug, PartialEq)]
pub enum SendError {
    /// Worth trying again later: network, 5xx, 401/403 (token being changed), 429.
    Retry(String),
    /// The server will never take this batch (other 4xx); drop it.
    Permanent(String),
}

impl SendError {
    pub fn from_status(status: u16, msg: String) -> Self {
        match status {
            401 | 403 | 408 | 429 => Self::Retry(msg),
            400..=499 => Self::Permanent(msg),
            _ => Self::Retry(msg),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Retry(m) | Self::Permanent(m) => m,
        }
    }
}

pub struct Queue {
    path: PathBuf,
}

impl Queue {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn is_empty(&self) -> bool {
        fs::metadata(&self.path).map_or(true, |m| m.len() == 0)
    }

    pub fn push(&self, item: &Item) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&self.path)
            && let Ok(line) = serde_json::to_string(item)
        {
            let _ = writeln!(f, "{line}");
        }
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > TRIM_BYTES) {
            self.trim();
        }
    }

    /// Drops the oldest lines beyond `MAX_LINES`.
    fn trim(&self) {
        let Ok(text) = fs::read_to_string(&self.path) else { return };
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        if lines.len() > MAX_LINES {
            self.rewrite(&lines[lines.len() - MAX_LINES..]);
        }
    }

    /// Sends the oldest batch together with `extra` (the open block), in one request.
    /// On success, or a permanent rejection (logged), that batch leaves the queue; on a
    /// retryable error everything stays. An empty queue still sends `extra`.
    pub fn flush(&self, extra: Vec<Item>, send: impl Fn(&UsageBatch) -> Result<(), SendError>) -> Result<(), String> {
        let text = fs::read_to_string(&self.path).unwrap_or_default();
        let mut lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        if lines.len() > MAX_LINES {
            lines.drain(..lines.len() - MAX_LINES);
        }
        let take = lines.len().min(FLUSH_BATCH);
        // Unreadable lines are dropped along with the batch.
        let mut batch = UsageBatch::default();
        for item in lines[..take].iter().filter_map(|l| serde_json::from_str::<Item>(l).ok()).chain(extra) {
            match item {
                Item::Usage(b) => batch.blocks.push(b),
                Item::Media(m) => batch.media.push(m),
            }
        }
        let result = if batch.blocks.is_empty() && batch.media.is_empty() {
            Ok(())
        } else {
            match send(&batch) {
                Ok(()) => Ok(()),
                Err(SendError::Permanent(e)) => {
                    log::error!("dropping {} queued blocks: {e}", take);
                    Ok(())
                }
                Err(SendError::Retry(e)) => return Err(e),
            }
        };
        if take > 0 || lines.len() < text.lines().count() {
            self.rewrite(&lines[take..]);
        }
        result
    }

    /// Replaces the file through a temp file and a rename, so a crash mid-write
    /// leaves either the old queue or the new one, never half of one.
    fn rewrite(&self, lines: &[&str]) {
        if lines.is_empty() {
            let _ = fs::remove_file(&self.path);
            return;
        }
        let tmp = tmp_path(&self.path);
        if fs::write(&tmp, lines.join("\n") + "\n").is_ok() {
            let _ = fs::rename(&tmp, &self.path);
        }
    }
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}
