-- The macOS account playing the media; joins the sort key as in mac_usage (V27). Old rows: ''.
ALTER TABLE mac_media
    ADD COLUMN IF NOT EXISTS user_name LowCardinality(String) AFTER device_name,
    MODIFY ORDER BY (device_id, start_at, user_name);
