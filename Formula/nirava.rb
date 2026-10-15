class Nirava < Formula
  desc "Native Wayland settings for Niri and Hyprland"
  homepage "https://github.com/Somchandra17/nirava"
  license "GPL-3.0-or-later"
  head "https://github.com/Somchandra17/nirava.git", branch: "main"

  depends_on :linux
  depends_on "pkgconf" => :build
  depends_on "rust" => :build
  depends_on "gtk4"
  depends_on "libadwaita"
  depends_on "pulseaudio"

  def fetch
    system "cargo", "fetch", "--locked"
  end

  def install
    system "cargo", "build", "--offline", "--locked", "--release", "--workspace",
           "--features", "nirava-service/audio,nirava-cli/audio"
    bin.install "target/release/nirava", "target/release/niravactl", "target/release/nirava-sessiond"
    (share/"applications").install "data/io.github.Somchandra17.Nirava.desktop"
    (share/"icons/hicolor/scalable/apps").install "data/io.github.Somchandra17.Nirava.svg"
    (share/"metainfo").install "data/io.github.Somchandra17.Nirava.metainfo.xml"
    # Examples only: installation never registers or enables a user service.
    (pkgshare/"examples").install "data/nirava-sessiond.service"
  end

  def caveats
    <<~EOS
      Linux/Wayland development preview, built from main with --HEAD.
      Run nirava --demo to preview, or nirava for read-only session discovery.
      Run nirava-sessiond in a terminal in your graphical login to provide
      read-only host discovery to the Flatpak frontend. No service is enabled.
      To expose the native launcher in desktop menus, add
        #{HOMEBREW_PREFIX}/share
      to your session's XDG_DATA_DIRS, or use tools/register-launcher.py from
      the source checkout with --prefix #{HOMEBREW_PREFIX}/opt/nirava.
      Homebrew owns these files; remove with brew uninstall nirava.
    EOS
  end

  test do
    require "json"
    state = JSON.parse(shell_output("#{bin}/niravactl status --demo --json"))
    assert_equal true, state.fetch("read_only")
    inventory = JSON.parse(shell_output("#{bin}/niravactl apps --demo --json"))
    assert_equal true, inventory.fetch("simulation")
    assert_match "Nothing was executed", shell_output("#{bin}/niravactl package-plan rpm install vlc")
  end
end
