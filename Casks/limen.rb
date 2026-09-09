cask "limen" do
  version "0.1.0"
  sha256 "b3d03019839cd5d9d3da5fae7eb0f27d1ef9c512776b2c0d0b1f8c49604a30fd"

  url "https://github.com/ranajoy-dutta/limen/releases/download/v#{version}/Limen_#{version}_aarch64.dmg"
  name "Limen"
  desc "Native macOS menu bar AWS credential switcher for IAM Identity Center"
  homepage "https://github.com/ranajoy-dutta/limen"

  depends_on macos: :monterey

  app "Limen.app"

  zap trash: [
    "~/.aws/credentials",
    "~/Library/Application Support/com.limen.desktop",
    "~/Library/Caches/com.limen.desktop",
    "~/Library/WebKit/com.limen.desktop",
  ]
end
