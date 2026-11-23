-- Screenshots taken (Biome Screenshots.Screenshot). Over events, read FINAL.
CREATE OR REPLACE VIEW screenshots AS
SELECT source_device, start_at AS at, fields['1.4.1'] AS path
FROM events FINAL
WHERE stream = 'biome:Screenshots.Screenshot';
