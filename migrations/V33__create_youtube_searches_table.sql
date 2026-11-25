-- Plays without a link that were searched for on YouTube by title and channel, once each.
-- video_id is empty when no single video matched exactly.
CREATE TABLE IF NOT EXISTS youtube_searches
(
    title       String,
    artist      String,
    video_id    String,
    searched_at DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(searched_at)
ORDER BY (title, artist);
