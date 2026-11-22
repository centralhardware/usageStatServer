-- One row per stretch of video playing on a Mac: the app holding the "keep the display awake"
-- power assertion and, for a browser, the domain of its front tab (source). Re-sent with a later
-- stop while it plays, like mac_usage: read with FINAL.
CREATE TABLE IF NOT EXISTS mac_media
(
    device_id   LowCardinality(String),
    device_name LowCardinality(String),
    source      LowCardinality(String),
    bundle_id   LowCardinality(String),
    app_name    LowCardinality(String),
    start_at    DateTime64(3, 'UTC'),
    stop_at     DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(stop_at)
PARTITION BY toYYYYMM(start_at)
ORDER BY (device_id, start_at);
