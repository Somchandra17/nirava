# Verification record

Checked on the user's Fedora 45 Niri desktop on 2026-10-15.

## Passed

- Rust CLI and service compile, including native audio support linked through
  libpulse-binding's supported runtime-library fallback.
- 26 unit tests: config preservation and round trips, backup/restore with
  concurrent-edit refusal, symlink refusal, Niri layout validation, simulated
  display watchdog/restart/hotplug recovery, command output bounds, stale audio
  generations, and channel balance.
- Private-bus integration: GetSnapshot, config/audio write refusal with and
  without --allow-writes, and normal SIGTERM shutdown. Temporary XDG paths
  remain empty. Test buses have no service directories or helper activation.
- Entire Rust workspace passes `DOCS_RS=1 cargo check`; this bypasses native
  linking, so it is a source/type check, not proof of a runnable window.
- Installed GTK/Adwaita runtime loads the GtkBuilder window template through
  Python GI. The window was not presented by this template check.
- Native window links through the local runtime-library fallback, runs all four
  demo pages, and renders wide/narrow scenes without GTK warnings. Screenshots
  of Plain's own rendered scene were inspected; other apps were not captured.
- Real GTK frontend and demo backend coexist on the isolated bus and exchange
  read-only snapshots. Their application/backend bus names are distinct.
- Read-only `plainctl doctor` matches Niri DP-5 at 3440x1440 / 159.96 Hz /
  scale 1, detects Noctalia, reads logind suspend=yes / hibernate=na, and finds
  four native audio devices on PipeWire.
- Desktop-entry validator passes.

## Pending / limitations

- Standard pkg-config build still needs GTK/libadwaita development packages.
  The verified local runtime fallback requires no installation. Full keyboard
  and screen-reader acceptance remain pending.
- rustfmt and clippy are absent. Formatting/lint checks have not run.
- The user-unit definition is present, but not installed or enabled. Static
  systemd verification reports the expected missing `/usr/bin/plain-sessiond`
  executable because no system deployment took place.
- Real audio mutations, caffeine effectiveness, Noctalia reload/Restore,
  secure lock-before-suspend, and live display changes have not been tested.
- Persistent display Save, multi-file integration, hypridle/kanshi editing,
  and laptop acceptance are still implementation work. v1 is not complete.

No desktop config, installed package, service enablement, audio setting, display
mode, or power state was changed by these checks. Source/build files and Cargo
download caches were created. This is a development preview, not a v1 release.
