-- One-off, after V34: drop a row lying inside the row before it with the same title, source and app.
ALTER TABLE mac_media DELETE WHERE (device_id, user_name, start_at) IN (
    SELECT device_id, user_name, start_at
    FROM (
        SELECT device_id, user_name, start_at, stop_at, title, source, bundle_id,
               lagInFrame(start_at, 1, toDateTime64(0, 3)) OVER w AS pstart, lagInFrame(stop_at, 1, toDateTime64(0, 3)) OVER w AS pstop,
               lagInFrame(title, 1, '') OVER w AS ptitle, lagInFrame(source, 1, '') OVER w AS psource, lagInFrame(bundle_id, 1, '') OVER w AS pbundle
        FROM mac_media FINAL
        WINDOW w AS (PARTITION BY device_id, user_name ORDER BY start_at ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)
    )
    WHERE title != '' AND title = ptitle AND source = psource AND bundle_id = pbundle AND start_at > pstart AND stop_at <= pstop
)
