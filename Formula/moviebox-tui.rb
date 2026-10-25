class MovieboxTui < Formula
  VERSION = "0.1.27"
  MACOS_SHA256 = "eb8c87fd8ed64f25fd5b3fbab50118e0eca9c393b96f089fa2c00b798243ec70"
  LINUX_X64_SHA256 = "a1f7372f533aa86fb0f878fad8908ae620e54d6ff61ef60452cd21e0f0a54705"
  LINUX_ARM64_SHA256 = "f69b1e75f99212fe8c61aae3eef266d3e7db0d150b1493b7303844bf78124705"

  desc "Stream movies, shows, anime, and live TV from your terminal"
  homepage "https://github.com/mesamirh/MovieBox-Tui"
  version VERSION
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_macOS_Universal.tar.gz"
    sha256 MACOS_SHA256
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_arm64.tar.gz"
      sha256 LINUX_ARM64_SHA256
    else
      url "https://github.com/mesamirh/MovieBox-Tui/releases/download/v#{VERSION}/MovieBox_Linux_x64.tar.gz"
      sha256 LINUX_X64_SHA256
    end
  end

  def install
    bin.install "moviebox-tui"
  end

  test do
    system "#{bin}/moviebox-tui", "--version"
  end
end
