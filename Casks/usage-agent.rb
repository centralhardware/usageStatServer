cask "usage-agent" do
  version "0.2.10"
  sha256 "980331e47c87197428bd98e827f6fcb9c22733449c1f6e56c55401ce1132a7b5"

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
