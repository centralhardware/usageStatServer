# UsageStatServer

Collects Mac app usage into ClickHouse: which app was in front (with its window title),
when nobody was at the Mac and why, and from that how often it was picked up.
The macOS agent that sends it lives in [`agent/`](agent/README.md).

## API

- `POST /api/usage`: `{"blocks": [...]}`, each a [`UsageBlock`](model/src/lib.rs) (snake_case,
  times in unix milliseconds), at most 1000 per request; answered with `201`, or `422` for an
  invalid block (nothing is saved then). When `API_TOKEN` is set, requests must carry
  `Authorization: Bearer <API_TOKEN>`.
- `GET /ping`: `200` if ClickHouse answers, `503` if not. The Docker health check uses it.

## Configuration

| env | default |
|---|---|
| `CH_URL` | `http://localhost:8123` |
| `CH_DB` | `default` |
| `CH_USER` / `CH_PASSWORD` | `default` / empty |
| `API_TOKEN` | unset (no auth, logged as a warning) |
| `YOUTUBE_API_KEY` | unset (YouTube lookups off) |
| `PORT` | `8080` |

## Schema

Table `mac_usage` (ReplacingMergeTree(stop_at), `ORDER BY (device_id, start_at)`), one row per stretch. `device_id` is the Mac's IOPlatformUUID (as in `battery_health`), `device_name` its name:

| kind | bundle_id / app_name | title |
|---|---|---|
| `app` | the frontmost app | its focused window's title (empty without Accessibility) |
| `away` | `idle` (no input for 5 min, back-dated to the last input), `lock`, `sleep` | empty |

The agent re-sends the block it is still in with a later `stop_at` every minute, so several rows
share a `(device_id, start_at)`; ClickHouse keeps the latest on merge. **Query with `FINAL`.**
A pickup is an `away` block with `bundle_id` `lock` or `sleep`.

```sql
SELECT app_name, round(sum(dateDiff('second', start_at, stop_at)) / 3600, 2) AS hours
FROM mac_usage FINAL
WHERE kind = 'app' AND start_at >= today()
GROUP BY app_name ORDER BY hours DESC
```

Columns `domain` (front browser tab, app blocks only) and `app_category` (the app's own
LSApplicationCategoryType, Screen Time style) feed classification.

Table `mac_media`: one row per stretch of video playing: `source` (the browser tab's domain, or
the player app's name), the app, start/stop. Also re-sent while open: query with `FINAL`.
`url` is the video's link when the playing tab is a YouTube video.

### YouTube

With `YOUTUBE_API_KEY` (YouTube Data API v3) the server looks up every video in `mac_media.url`
(`youtube_videos`) and its channel (`youtube_channels`: link, handle, country, subscribers, topics…;
refreshed weekly). View `youtube_watched` joins them to what played.

### Classification (RescueTime style)

`activity_rules` maps an app (`kind = 'app'`, pattern = bundle id) or a site (`kind = 'domain'`)
to a category and a productivity level from -2 (very distracting) to +2 (very productive).
It starts empty, with no built-in site list; add rules as you go:

```sql
INSERT INTO activity_rules (kind, pattern, category, productivity) VALUES
    ('domain', 'youtube.com', 'Entertainment', -2),
    ('app', 'com.jetbrains.intellij', 'Development', 2);
```

A domain rule beats an app rule, and both beat the app's declared category:

```sql
SELECT
    if(d.category != '', d.category, if(a.category != '', a.category,
       replaceOne(u.app_category, 'public.app-category.', ''))) AS category,
    if(d.category != '', d.productivity, a.productivity) AS productivity,
    round(sum(dateDiff('second', u.start_at, u.stop_at)) / 3600, 2) AS hours
FROM (SELECT * FROM mac_usage FINAL WHERE kind = 'app' AND start_at >= today()) AS u
LEFT JOIN (SELECT * FROM activity_rules FINAL WHERE kind = 'domain') AS d ON d.pattern = u.domain
LEFT JOIN (SELECT * FROM activity_rules FINAL WHERE kind = 'app') AS a ON a.pattern = u.bundle_id
GROUP BY category, productivity ORDER BY hours DESC
```

History is in [`migrations/`](migrations/); the server applies new ones at startup.

## Deploy

Pushing to `main` builds `ghcr.io/centralhardware/usagestatserver` and triggers the Coolify
deploy hook. Changes that touch only `agent/` or docs don't trigger a deploy.

## Window title parsing

The server splits each title into `subject` (what you're on: file, chat, page, command) and
`context` (where: project, account, site, host); any other named group in the matching rule
(`(?P<host>…)`, `(?P<branch>…)`) lands in the `fields` map (`fields['host']`). The raw `title` is kept. Rules live in
`title_rules` (a regex with named groups `subject`/`context`, per bundle id; `''` = any app;
lower `priority` first). Migration V9 seeds them for IntelliJ, Terminal, Telegram, Safari,
Chrome and Mail; apps without a rule get a generic split on ` — `, ` – `, ` | `, ` - `, ` @ `.
Titles are cleaned first (invisible marks, trailing `(3)` counters, `207×59` sizes).
The server reloads the rules every 5 minutes and re-parses stored titles when they change:

```sql
INSERT INTO usage_stats.title_rules (bundle_id, pattern) VALUES
    ('com.microsoft.VSCode', '^(?P<subject>.+?) — (?P<context>.+?)$');
```
