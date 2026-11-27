-- One-off: the 2-4 s rows without a title while the next episode loaded (before agent 0.2.21).
ALTER TABLE mac_media DELETE WHERE title = '' AND source != 'Private Browsing' AND dateDiff('millisecond', start_at, stop_at) < 10000
