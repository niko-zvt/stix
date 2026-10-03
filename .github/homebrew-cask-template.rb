# Homebrew Cask Template for Stix
#
# CI does not publish this cask. To offer a tap later:
#   1. Create a new repo: github.com/niko-zvt/homebrew-stix
#   2. Place this file at: Casks/stix.rb
#   3. After each release, update `version` and `sha256`
#   4. Users install with: brew install --cask niko-zvt/stix/stix
#
# To calculate SHA256 after a release:
#   shasum -a 256 Stix_<version>_aarch64.dmg
#   shasum -a 256 Stix_<version>_x64.dmg

cask "stix" do
  arch arm: "aarch64", intel: "x64"

  version "0.3.0"
  sha256 arm:   "REPLACE_WITH_ARM64_SHA256",
         intel: "REPLACE_WITH_X64_SHA256"

  url "https://github.com/niko-zvt/stix/releases/download/v#{version}/Stix_#{version}_#{arch}.dmg"
  name "Stix"
  desc "Instant thought capture - one shortcut, post-it appears, type, close"
  homepage "https://github.com/niko-zvt/stix"

  depends_on macos: :sonoma

  app "Stix.app"

  zap trash: [
    "~/Documents/Stix",
    "~/.stix",
    "~/Library/Caches/com.stix.app",
    "~/Library/WebKit/com.stix.app",
  ]
end
