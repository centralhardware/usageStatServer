-- Documents opened (Biome App.DocumentInteraction). Over events, read FINAL.
CREATE OR REPLACE VIEW documents_opened AS
SELECT source_device, start_at AS at, fields['2.1'] AS path, fields['3'] AS type, fields['4.1'] AS app
FROM events FINAL
WHERE stream = 'biome:App.DocumentInteraction';
