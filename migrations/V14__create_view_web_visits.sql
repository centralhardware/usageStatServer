-- Safari page visits with full URL (Biome App.WebUsage); state 2 = page shown, 3 = left. Over events, read FINAL.
CREATE OR REPLACE VIEW web_visits AS
SELECT source_device, start_at AS at, fields['1'] AS tab_session, toUInt8OrZero(fields['3']) AS state, fields['4'] AS url, fields['5'] AS domain, fields['6'] AS app
FROM events FINAL
WHERE stream = 'biome:App.WebUsage';
