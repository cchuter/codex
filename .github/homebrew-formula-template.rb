class Osmiflow < Formula
  desc "AI-powered coding assistant built on Codex"
  homepage "https://github.com/cchuter/codex"
  version "VERSION_PLACEHOLDER"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/cchuter/codex/releases/download/vVERSION_PLACEHOLDER/osmiflow-darwin-arm64.tar.gz"
      sha256 "SHA256_DARWIN_ARM64_PLACEHOLDER"
    else
      url "https://github.com/cchuter/codex/releases/download/vVERSION_PLACEHOLDER/osmiflow-darwin-amd64.tar.gz"
      sha256 "SHA256_DARWIN_AMD64_PLACEHOLDER"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/cchuter/codex/releases/download/vVERSION_PLACEHOLDER/osmiflow-linux-amd64.tar.gz"
      sha256 "SHA256_LINUX_AMD64_PLACEHOLDER"
    else
      odie "osmiflow is not available for Linux ARM architecture"
    end
  end

  def install
    bin.install "osmiflow"
  end

  test do
    assert_match "osmiflow", shell_output("#{bin}/osmiflow --version")
  end
end