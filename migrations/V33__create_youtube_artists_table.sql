-- Artists of YouTube plays without a video link, searched for as a channel of exactly that name,
-- once each. channel_id is empty when there is none; guess is 1 when several channels have the
-- name (the one with the most subscribers is taken). Read with FINAL.
CREATE TABLE IF NOT EXISTS youtube_artists
(
    artist      String,
    channel_id  String,
    guess       UInt8,
    searched_at DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(searched_at)
ORDER BY artist;
