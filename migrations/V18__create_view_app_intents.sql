-- Actions apps reported to the system (Biome App.Intent): intent class, verb, target handle. Over events, read FINAL.
CREATE OR REPLACE VIEW app_intents AS
SELECT source_device, start_at AS at, fields['2'] AS app, fields['4'] AS intent_class, fields['5'] AS verb, fields['12'] AS target
FROM events FINAL
WHERE stream = 'biome:App.Intent';
