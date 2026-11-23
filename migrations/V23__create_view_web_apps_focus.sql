-- Safari web apps in focus (Biome App.WebApp.InFocus); starting 1 = gained focus. Over events, read FINAL.
CREATE OR REPLACE VIEW web_apps_focus AS
SELECT source_device, start_at AS at, toUInt8OrZero(fields['1']) AS starting, fields['5'] AS name, fields['6'] AS url, fields['4'] AS web_app_id
FROM events FINAL
WHERE stream = 'biome:App.WebApp.InFocus';
