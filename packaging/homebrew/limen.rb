cask "limen" do
  version "0.1.0"
  sha256 "REPLACE_WITH_SHA256"

  url "https://github.com/ranajoy-dutta/limen/releases/download/v#{version}/Limen_#{version}_aarch64.dmg"
  name "Limen"
  desc "Native macOS menu bar AWS credential switcher for IAM Identity Center"
  homepage "https://github.com/ranajoy-dutta/limen"

  depends_on macos: ">= :monterey"

  app "Limen.app"

  zap trash: [
    "~/.aws/credentials",
    "~/Library/Application Support/com.limen.desktop",
    "~/Library/Caches/com.limen.desktop",
    "~/Library/WebKit/com.limen.desktop",
  ]
end
