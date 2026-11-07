# typed: strict
# frozen_string_literal: true

# Lightweight YouTube Music client for the terminal (InnerTube + mpv).
class Termtube < Formula
  desc "Lightweight YouTube Music client for the terminal"
  homepage "https://javierlongoriaroche.github.io/termtube/"
  url "https://github.com/javierlongoriaroche/termtube/archive/refs/tags/v0.2.2.tar.gz"
  sha256 "8b2df2f77a670eae993a031a91eec03c46ec8b541fea7112a983262bcda4b829"
  license "MIT"
  head "https://github.com/javierlongoriaroche/termtube.git", branch: "master"

  depends_on "rust" => :build
  depends_on "mpv"
  depends_on "yt-dlp"

  def install
    system "cargo", "install", *std_cargo_args
  end

  def caveats
    <<~EOS
      Run `termtube`: it finds your YouTube Music session in your browser,
      or opens music.youtube.com so you can sign in.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/termtube --version")
  end
end
