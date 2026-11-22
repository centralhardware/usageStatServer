-- One row per stretch on a Mac: an app in front ('app', with its window title) or nobody
-- at the Mac ('away', bundle_id = idle/lock/sleep). The agent re-sends the open stretch with a
-- later stop, so the row with the latest stop per (device, start) wins: read with FINAL.
CREATE TABLE IF NOT EXISTS mac_usage
(
    device_id LowCardinality(String),
    kind      LowCardinality(String),
    bundle_id LowCardinality(String),
    app_name  LowCardinality(String),
    title     LowCardinality(String),
    start_at  DateTime64(3, 'UTC'),
    stop_at   DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(stop_at)
PARTITION BY toYYYYMM(start_at)
ORDER BY (device_id, start_at);
