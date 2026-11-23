-- Media playing per app (Biome App.MediaUsage); starting 1 = started. Over events, read FINAL.
CREATE OR REPLACE VIEW media_usage_biome AS
SELECT source_device, start_at AS at, fields['2'] AS app, toUInt8OrZero(fields['1']) AS starting
FROM events FINAL
WHERE stream = 'biome:App.MediaUsage';
