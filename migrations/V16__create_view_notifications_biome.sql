-- Notifications seen/opened (Biome Notification.Usage); kind is Apple's usage type. Over events, read FINAL.
CREATE OR REPLACE VIEW notifications_biome AS
SELECT source_device, start_at AS at, fields['4'] AS app, toUInt8OrZero(fields['3']) AS kind, fields['5'] AS notification_id
FROM events FINAL
WHERE stream = 'biome:Notification.Usage';
