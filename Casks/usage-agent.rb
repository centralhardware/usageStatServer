cask "usage-agent" do
  version "0.2.14"
  sha256 "218c3540d36cbbbc8eb074a243a0785d38ff1d2ec84cf0cb27b92f5820629a4f"

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
