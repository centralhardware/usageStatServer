-- Screen Time's own app focus log (Biome ScreenTime.AppUsage); starting 1 = gained focus. Over events, read FINAL.
CREATE OR REPLACE VIEW screentime_app_usage AS
SELECT source_device, start_at AS at, fields['3'] AS app, toUInt8OrZero(fields['1']) AS starting
FROM events FINAL
WHERE stream = 'biome:ScreenTime.AppUsage';
