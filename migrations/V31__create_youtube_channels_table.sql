-- The channels of youtube_videos, from the Data API, refreshed weekly. Subscribers are rounded by
-- YouTube, and 0 when the channel hides them. Read with FINAL.
CREATE TABLE IF NOT EXISTS youtube_channels
(
    channel_id    String,
    url           String,
    handle        String,
    title         String,
    description   String,
    country       LowCardinality(String),
    keywords      String,
    topics        Array(String),
    thumbnail_url String,
    subscribers   UInt64,
    views         UInt64,
    videos        UInt64,
    published_at  String,
    fetched_at    DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(fetched_at)
ORDER BY channel_id;
