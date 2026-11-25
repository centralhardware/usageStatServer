-- YouTube videos seen in mac_media.url, from the Data API. A video it doesn't return (deleted,
-- private) is kept with an empty channel_id so it isn't asked for again. Read with FINAL.
CREATE TABLE IF NOT EXISTS youtube_videos
(
    video_id     String,
    channel_id   String,
    title        String,
    description  String,
    category_id  LowCardinality(String),
    tags         Array(String),
    duration     LowCardinality(String),
    views        UInt64,
    likes        UInt64,
    published_at String,
    fetched_at   DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(fetched_at)
ORDER BY video_id;
