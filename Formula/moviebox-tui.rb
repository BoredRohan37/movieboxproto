class MovieboxTui < Formula
  VERSION = "0.1.27"
  MACOS_SHA256 = "94302aa1c3c68ffb12e8762c39096da137e49c0b36c0c727437c510a7273fa97"
  LINUX_X64_SHA256 = "609b1bf08b13c95b1e123346efcaed73209e444d783d0019342eba0c5d13c17d"
  LINUX_ARM64_SHA256 = "2ea8b194b104f0767499f9d36f03bab2a7f0f68eac0e26ea2f779ffb8e26872b"

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
