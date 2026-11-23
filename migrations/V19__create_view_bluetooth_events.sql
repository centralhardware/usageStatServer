-- Bluetooth device events on every synced device (Biome Device.Wireless.Bluetooth); connected 1/0. Over events, read FINAL.
CREATE OR REPLACE VIEW bluetooth_events AS
SELECT source_device, start_at AS at, fields['1'] AS address, fields['2'] AS device_name, toUInt8OrZero(fields['4']) AS connected
FROM events FINAL
WHERE stream = 'biome:Device.Wireless.Bluetooth';
