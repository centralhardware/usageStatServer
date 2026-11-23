-- Menu commands used (Biome App.MenuItem). Over events, read FINAL.
CREATE OR REPLACE VIEW menu_items AS
SELECT source_device, start_at AS at, fields['1'] AS app, fields['3'] AS item
FROM events FINAL
WHERE stream = 'biome:App.MenuItem';
