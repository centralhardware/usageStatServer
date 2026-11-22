-- device_id becomes the Mac's IOPlatformUUID (as in battery_health); its human name moves here.
ALTER TABLE mac_usage ADD COLUMN IF NOT EXISTS device_name LowCardinality(String) DEFAULT '' AFTER device_id;
