-- One-off: a play split by a pause of under 10 s (before agent 0.2.21) becomes one row. The first
-- row is re-inserted with the end of the last one; ReplacingMergeTree(stop_at) keeps the longer.
-- V35 then drops the later rows, which now lie inside it. The 1-hour block cuts (no gap) stay.
INSERT INTO mac_media
SELECT * REPLACE (x.last AS stop_at)
FROM mac_media AS m FINAL
INNER JOIN (
    SELECT device_id AS d, user_name AS u, min(start_at) AS first, max(stop_at) AS last
    FROM (
        SELECT device_id, user_name, start_at, stop_at,
               sum(NOT cont) OVER (PARTITION BY device_id, user_name ORDER BY start_at ROWS UNBOUNDED PRECEDING) AS grp
        FROM (
            SELECT device_id, user_name, start_at, stop_at,
                   title != '' AND title = lagInFrame(title, 1, '') OVER w AND source = lagInFrame(source, 1, '') OVER w
                   AND bundle_id = lagInFrame(bundle_id, 1, '') OVER w
                   AND dateDiff('millisecond', lagInFrame(stop_at, 1, toDateTime64(0, 3)) OVER w, start_at) BETWEEN 1 AND 9999 AS cont
            FROM mac_media FINAL
            WINDOW w AS (PARTITION BY device_id, user_name ORDER BY start_at ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)
        )
    )
    GROUP BY device_id, user_name, grp
    HAVING count() > 1
) AS x ON m.device_id = x.d AND m.user_name = x.u AND m.start_at = x.first
