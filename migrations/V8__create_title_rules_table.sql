-- How to split an app's window titles: a regex (Rust `regex` syntax) with the named groups
-- `subject` and/or `context` (any other named group lands in mac_usage.fields), matched against the cleaned title (invisible marks, trailing
-- "(3)" counters and "207×59" sizes removed). bundle_id '' applies to every app. Lower priority
-- is tried first; the first matching rule wins, else the generic separator split.
-- The server reloads the rules every 5 minutes and re-parses stored titles when they change.
-- Re-inserting (bundle_id, pattern) replaces it: read with FINAL.
CREATE TABLE IF NOT EXISTS title_rules
(
    bundle_id  LowCardinality(String),
    pattern    String,
    priority   Int16 DEFAULT 100,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (bundle_id, pattern);
