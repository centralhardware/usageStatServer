# usage-agent

A macOS LaunchAgent (per user: it needs the GUI session) that every second samples the
frontmost app, its focused window's title, input idle time and the lock state, turns that into
blocks and posts them to `POST /api/usage` every minute. It stays resident (launchd `KeepAlive`).

- A new app or a new window title starts a new block; blocks under 2 s are dropped.
- A Safari private window is logged with the title `Private Browsing` and no domain (detected from
  Safari's Window menu, which offers "Move Tab to New Private Window" only there; English menus only).
  Video playing while the front Safari window is private gets source `Private Browsing`, no title.
- For a browser, the front tab's domain is recorded too (asked via AppleScript when the title
  changes; macOS asks once per browser to allow Automation). Each app's declared category
  (LSApplicationCategoryType) is recorded with its blocks.
- Every second it checks what plays video (the app holding a display-sleep power assertion) and,
  for a browser, the front tab's domain when playback starts (kept while it plays on, since
  browsers don't say which tab plays); those stretches go to `mac_media`. When the tab titled like the
  playing media is a YouTube video, its link is sent too (no other address is sent with media).
- No input for 5 min, a locked screen or sleep (a gap between samples) is an `away` block with that reason.
- The open block is re-sent with a later stop on every flush; blocks the server hasn't taken wait in
  `~/Library/Application Support/usage-agent/queue.jsonl`.

## Install

```sh
brew tap centralhardware/usage-agent https://github.com/centralhardware/usageStatServer
brew install --cask usage-agent
```

or download the `.dmg` from the `agent-v*` releases and run **Install usage-agent.pkg**. It installs:

- `/usr/local/libexec/usage-agent`: a universal binary (Apple Silicon and Intel), signed with LocalCodeSign
- `/Library/LaunchAgents/me.centralhardware.usage-agent.plist`

The installer asks for the server URL and API token on first install. On first run macOS asks to
allow **Accessibility** for `usage-agent` (System Settings → Privacy & Security): without it,
window titles stay empty.

Uninstall with `brew uninstall --cask usage-agent` or **Uninstall usage-agent.command** from the dmg.

## Configure

Per user, re-read every minute (no restart needed):

    defaults write me.centralhardware.usage-agent ServerURL https://host
    defaults write me.centralhardware.usage-agent AuthToken -string <token>

`USAGE_AGENT_URL` / `USAGE_AGENT_TOKEN` override them for a manual run.

## Debug

    /usr/local/libexec/usage-agent --dry-run     # one sample: app, title, idle, locked, accessibility
    tail -f /Users/Shared/usage-agent.log
    launchctl kickstart -k gui/$(id -u)/me.centralhardware.usage-agent

## Build

    cd agent && ./packaging/build-dmg.sh   # on macOS; writes target/usage-agent-<version>.dmg
