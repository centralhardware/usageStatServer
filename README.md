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
| `PORT` | `8080` |

## Schema

Table `mac_usage` (ReplacingMergeTree(stop_at), `ORDER BY (device_id, start_at)`), one row per stretch:

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

History is in [`migrations/`](migrations/); new migrations are applied by hand before deploying
the code that needs them.

## Deploy

Pushing to `main` builds `ghcr.io/centralhardware/usagestatserver` and triggers the Coolify
deploy hook. Changes that touch only `agent/`, `migrations/` or docs don't trigger a deploy.
