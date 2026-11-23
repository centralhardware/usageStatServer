# usage-agent for PocketBook

Records reading on a PocketBook (firmware 6.x, B288 SDK) into the same `mac_usage` table as the
macOS agent, through `POST /api/usage`:

| kind | bundle_id / app_name | title |
|---|---|---|
| `app` | the foreground app (e.g. `eink-reader`, `bookshelf`, `explorer`) | the open book's file name without extensions, only while its reader is in front |
| `away` | `sleep` (device suspended) | empty |

It is a C program that stays resident as a background InkView task: tapping **usage-agent** in
Applications starts it (once per boot) and shows its status; tapping again shows the status and
sends it back to the background. Every 30 s it asks:

- the firmware task manager (`GetActiveTask`/`GetTaskInfo`) which app is in front. It answers only
  a registered InkView task, which is why the agent isn't a forked daemon; readers don't take the
  book on their command line and `/var/run/task/<pid>` mtimes don't follow switching either;
- the book: the one the active subtask registered, else `/tmp/.current` (written by the firmware
  on open) while the app that was in front when it changed stays in front;
- a jump of wall-clock time past monotonic time: the device slept.

Closed blocks are spooled to a file and sent at start, every 5 minutes and on waking, retrying
every 30 s for 5 minutes after a failure (Wi-Fi takes a while to come back after sleep). The
agent never turns Wi-Fi on itself.

## Install

1. Download `usage-agent.zip` from the latest `pocketbook-agent-v*` release, unzip it and copy
   `usage-agent.app` to `applications/` on the device over USB.
2. Create `system/usage-agent/config`:
   ```
   server_url=https://usage.centralhardware.me
   token=<API_TOKEN>
   device_id=pocketbook-619
   device_name=PocketBook 619
   ```
3. Eject, open Applications → usage-agent. After a reboot, tap it again. Closing it in the task
   manager stops it.

Files in `system/usage-agent/`: `spool.jsonl` (unsent blocks), `log`.

## Build

`make test` runs the host unit tests; `make SDK=<SDK_6.3.0>/SDK-B288` cross-compiles (CI does both
and uploads the binary).
