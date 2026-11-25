-- What played on YouTube, with its video and channel. Columns stay empty until the lookup is done.
CREATE OR REPLACE VIEW youtube_watched AS
SELECT m.device_id AS device_id, m.device_name AS device_name, m.user_name AS user_name,
       m.start_at AS start_at, m.stop_at AS stop_at,
       m.title AS title, m.artist AS artist, m.url AS video_url, v.video_id AS video_id,
       v.duration AS video_duration, parseDateTimeBestEffortOrNull(v.published_at) AS video_published_at,
       v.views AS video_views, v.likes AS video_likes, v.tags AS video_tags,
       c.channel_id AS channel_id, c.url AS channel_url, c.handle AS channel_handle, c.title AS channel_title,
       c.description AS channel_description, c.country AS channel_country, c.topics AS channel_topics,
       c.thumbnail_url AS channel_thumbnail_url, c.subscribers AS channel_subscribers,
       c.views AS channel_views, c.videos AS channel_videos,
       parseDateTimeBestEffortOrNull(c.published_at) AS channel_created_at
FROM (SELECT *, extract(url, 'v=([A-Za-z0-9_-]{11})') AS id FROM mac_media FINAL WHERE url != '') AS m
LEFT JOIN (SELECT * FROM youtube_videos FINAL) AS v ON v.video_id = m.id
LEFT JOIN (SELECT * FROM youtube_channels FINAL) AS c ON c.channel_id = v.channel_id;
