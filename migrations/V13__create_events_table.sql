-- Events read by the agent from other macOS stores (knowledgeC: notifications, screen on/off,
-- media, app intents, Bluetooth). `value` is the stream's own value; `fields` keeps every other
-- non-empty attribute. Re-reads can repeat an event: read with FINAL.
CREATE TABLE IF NOT EXISTS events
(
    device_id     LowCardinality(String),
    source_device LowCardinality(String),
    stream        LowCardinality(String),
    bundle_id     LowCardinality(String),
    value         String,
    fields        Map(LowCardinality(String), String),
    start_at      DateTime64(3, 'UTC'),
    stop_at       DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(stop_at)
PARTITION BY toYYYYMM(start_at)
ORDER BY (source_device, stream, start_at, bundle_id, value);
