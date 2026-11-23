-- No DEFAULT: ClickHouse refuses a sort-key column added with one; a String defaults to '' anyway.
-- The macOS account the agent runs in: two accounts on one Mac share device_id, so it joins the
-- sort key, or their blocks starting in the same millisecond would replace each other. Old rows: ''.
ALTER TABLE mac_usage
    ADD COLUMN IF NOT EXISTS user_name LowCardinality(String) AFTER device_name,
    MODIFY ORDER BY (device_id, start_at, user_name);
