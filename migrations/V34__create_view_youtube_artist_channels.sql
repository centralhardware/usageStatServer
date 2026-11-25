-- The YouTube channel of each artist in mac_media: from a play with a link, else found by name.
CREATE OR REPLACE VIEW youtube_artist_channels AS
SELECT a.artist AS artist, a.by_name AS by_name, a.guess AS guess, c.channel_id AS channel_id, c.url AS url,
       c.handle AS handle, c.title AS title, c.description AS description, c.country AS country,
       c.topics AS topics, c.thumbnail_url AS thumbnail_url, c.subscribers AS subscribers
FROM
(
    SELECT toString(artist) AS artist, argMax(channel_id, start_at) AS channel_id, toUInt8(0) AS by_name, toUInt8(0) AS guess
    FROM youtube_watched WHERE channel_id != '' GROUP BY artist
    UNION ALL
    SELECT artist, channel_id, toUInt8(1) AS by_name, guess
    FROM youtube_artists FINAL
    WHERE channel_id != '' AND artist NOT IN (SELECT toString(artist) FROM youtube_watched WHERE channel_id != '')
) AS a
INNER JOIN (SELECT * FROM youtube_channels FINAL) AS c ON c.channel_id = a.channel_id;
