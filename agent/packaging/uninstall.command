#!/bin/sh
# Removes usage-agent. Double-click, or run with sudo.
[ "$(id -u)" -eq 0 ] || exec sudo "$0" "$@"
CONSOLE_USER=$(/usr/bin/stat -f %Su /dev/console)
/bin/launchctl bootout "gui/$(/usr/bin/id -u "$CONSOLE_USER")/me.centralhardware.usage-agent" 2>/dev/null
rm -f /Library/LaunchAgents/me.centralhardware.usage-agent.plist \
      /usr/local/libexec/usage-agent \
      /Users/Shared/usage-agent.log
/usr/sbin/pkgutil --forget me.centralhardware.usage-agent 2>/dev/null
echo "usage-agent removed."
