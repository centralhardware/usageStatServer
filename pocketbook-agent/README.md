# usage-agent for PocketBook

Records reading on a PocketBook (firmware 6.x, B288 SDK) into the same `mac_usage` table as the
macOS agent, through `POST /api/usage`:

| kind | bundle_id / app_name | title |
|---|---|---|
| `app` | the foreground app (e.g. `eink-reader`, `bookshelf`, `explorer`) | the open book's file name without extensions, only while its reader is in front |
| `away` | `sleep` (device suspended) | empty |

It is a C program: tapping **usage-agent** in Applications starts a background daemon (once per
boot), tapping it again shows its status. Every 30 s the daemon looks at:

- `/var/run/task/<pid>`: the firmware's task manager touches a task's file when it comes to the
  front, so the newest one of a live process is the app on screen (its name from `/proc`);
- `/tmp/.current`: the path of the book last opened, written on open. The task that came to the
  front within 15 s of that write is the reader; while it stays in front, that book is open;
- a jump of wall-clock time past monotonic time: the device slept.

(The InkView task API — `GetActiveTask`/`GetTaskInfo` — returns nothing outside an InkView app,
and readers don't take the book on their command line, hence these files.)

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
3. Eject, open Applications → usage-agent. After a reboot, tap it again.

Files in `system/usage-agent/`: `spool.jsonl` (unsent blocks), `log`, `pid`.

## Build

`make test` runs the host unit tests; `make SDK=<SDK_6.3.0>/SDK-B288` cross-compiles (CI does both
and uploads the binary).
