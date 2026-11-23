# usage-agent for PocketBook

Records reading on a PocketBook (firmware 6.x, B288 SDK) into the same `mac_usage` table as the
macOS agent, through `POST /api/usage`:

| kind | bundle_id / app_name | title |
|---|---|---|
| `app` | the reader process (e.g. `reader`) | book file name without extension |
| `app` | `library` / `Library` (no document open) | empty |
| `away` | `sleep` (device suspended) | empty |

It is a C program: tapping **usage-agent** in Applications starts a background daemon (once per
boot), tapping it again shows its status. Every 30 s the daemon scans `/proc` for a process that
opened a document under `/mnt/ext1` or `/mnt/ext2`; a jump of wall-clock time past monotonic time
means the device slept. Closed blocks are spooled to a file and sent every 5 minutes (and on
waking) whenever Wi-Fi happens to be up; the agent never turns Wi-Fi on itself.

## Install

1. Download `usage-agent.app` from the workflow artifact and copy it to `applications/` on the
   device over USB.
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
