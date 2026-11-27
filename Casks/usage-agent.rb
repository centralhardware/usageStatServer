cask "usage-agent" do
  version "0.2.21"
  sha256 "ea24ebabd65424cdef40557b2613b87807476b1842c9bb09cdf6ae1d5bc18bf8"

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
