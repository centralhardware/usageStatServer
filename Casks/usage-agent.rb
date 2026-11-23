cask "usage-agent" do
  version "0.2.11"
  sha256 "a5697c827d88c0a603b294287713ab2f3e33cd49cbb6021fdefece27a9d746fa"

  url "https://github.com/centralhardware/usageStatServer/releases/download/agent-v#{version}/usage-agent-#{version}.dmg"
  name "usage-agent"
  desc "Reports Mac app usage to UsageStatServer"
  homepage "https://github.com/centralhardware/usageStatServer"

  pkg "Install usage-agent.pkg"

  # Mirrors packaging/uninstall.command.
  uninstall launchctl: "me.centralhardware.usage-agent",
            pkgutil:   "me.centralhardware.usage-agent",
            delete:    "/Users/Shared/usage-agent.log"

  zap trash: "~/Library/Preferences/me.centralhardware.usage-agent.plist"
end
