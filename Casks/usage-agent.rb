cask "usage-agent" do
  version "0.2.13"
  sha256 "5eaf192905d7c359efa4b4d138492b2aaa474a315ba525e8247a497f226456e2"

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
