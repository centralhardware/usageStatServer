-- Stretches read by the agent from macOS Screen Time stores: apps in focus on every synced device
-- (Biome App.InFocus, incl. iPhone/iPad) and time on websites (knowledgeC webUsage). The agent
-- re-reads the stores, so the same stretch can arrive again: read with FINAL.
CREATE TABLE IF NOT EXISTS screen_time
(
    device_id     LowCardinality(String),
    source_device LowCardinality(String),
    kind          LowCardinality(String),
    bundle_id     LowCardinality(String),
    domain        String,
    start_at      DateTime64(3, 'UTC'),
    stop_at       DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(stop_at)
PARTITION BY toYYYYMM(start_at)
ORDER BY (source_device, kind, start_at, bundle_id, domain);
