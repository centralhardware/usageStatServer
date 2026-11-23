-- Notifications delivered (Biome Notification.Delivery). Over events, read FINAL.
CREATE OR REPLACE VIEW notification_deliveries AS
SELECT source_device, start_at AS at, fields['2'] AS app, fields['1'] AS notification_id
FROM events FINAL
WHERE stream = 'biome:Notification.Delivery';
