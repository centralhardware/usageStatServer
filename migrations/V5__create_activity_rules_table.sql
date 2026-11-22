-- Hand-maintained classification, RescueTime style: an app (kind 'app', pattern = bundle id)
-- or a site (kind 'domain', pattern = domain) → category and productivity (-2 very distracting
-- … +2 very productive). Re-inserting a pattern replaces it (latest updated_at wins): read with FINAL.
CREATE TABLE IF NOT EXISTS activity_rules
(
    kind         LowCardinality(String),
    pattern      String,
    category     LowCardinality(String),
    productivity Int8,
    updated_at   DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY (kind, pattern);
