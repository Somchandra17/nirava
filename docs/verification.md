# Verification record

Checked on the user's Fedora 45 Niri desktop on 2026-10-15.

## Passed

- Rust CLI and service compile, including native audio support linked through
  libpulse-binding's supported runtime-library fallback.
- 44 unit tests: config preservation and round trips, backup/restore with
  concurrent-edit refusal, symlink refusal, Niri layout validation, simulated
  display watchdog/restart/hotplug recovery, command output bounds, stale audio
  generations, channel balance, package identifier/command policy, exact RPM
  removal versions, Flatpak formats/scopes, staging conflicts, ancestor/backup
  symlinks, malformed idle input, aliased output edits, delayed display
  confirmation, transient recovery failure, and expired queued audio writes.
- Private-bus integration: GetSnapshot, config/audio write refusal with and
  without --allow-writes, and normal SIGTERM shutdown. Temporary XDG paths
  remain empty. Fictional Apps inventory is also checked. Test buses have no
  service directories or helper activation.
- Entire Rust workspace passes `DOCS_RS=1 cargo check`; this bypasses native
  linking, so it is a source/type check, not proof of a runnable window.
- Installed GTK/Adwaita runtime loads the GtkBuilder window template through
  Python GI. The window was not presented by this template check.
- Native window links through the local runtime-library fallback, runs all four
  original demo pages; the new Apps page also renders at 980/540 pixels.
  Isolated frontend tests disable accessibility only on the test bus (GTK_A11Y),
  which has no accessibility service; normal app accessibility stays enabled.
  Screenshots
  of Nirava's own rendered scene were inspected; other apps were not captured.
- Real GTK frontend and demo backend coexist on the isolated bus and exchange
  read-only snapshots. Their application/backend bus names are distinct.
- Read-only `niravactl doctor` matches Niri DP-5 at 3440x1440 / 159.96 Hz /
  scale 1, detects Noctalia, reads logind suspend=yes / hibernate=na, and finds
  four native audio devices on PipeWire.
- Desktop-entry validator and build-helper shell syntax checks pass.
- Read-only Apps inventory matched 2,782 RPM records, one user Flatpak and 13
  default-system Flatpaks, with three configured scoped remotes and no errors.
  Counts are a point-in-time observation, not a supported-machine requirement.
- Apps screenshots at 980/540 pixels were visually inspected. Native frontend
  tests exercise all five pages against an isolated demo backend.

## Pending / limitations

- Standard pkg-config build still needs GTK/libadwaita development packages.
  The verified local runtime fallback requires no installation. Full keyboard
  and screen-reader acceptance remain pending.
- rustfmt and clippy are absent. Formatting/lint checks have not run.
- The user-unit definition is present, but not installed or enabled. Static
  systemd verification reports the expected missing `/usr/bin/nirava-sessiond`
  executable because no system deployment took place.
- Real app launches and package transaction/terminal acceptance remain pending.
- Real audio mutations, caffeine effectiveness, Noctalia reload/Restore,
  secure lock-before-suspend, and live display changes have not been tested.
- Persistent display Save, multi-file integration, hypridle/kanshi editing,
  and laptop acceptance are still implementation work. v1 is not complete.

No desktop config, installed package, service enablement, audio setting, display
mode, or power state was changed by these checks. Source/build files and Cargo
download caches were created. This is a development preview, not a v1 release.

## Packaging checks (2026-10-15)

- Confirmed no Nirava binary on PATH, Flatpak installation or enabled user unit.
- Homebrew formula parses with Ruby and loads through the installed Homebrew
  formula DSL. Dependencies were inspected, not installed. Actual formula build
  and installation remain pending. Homebrew developer mode was restored to its
  original disabled state after the parser command automatically enabled it.
- Four packaging tests cover default plan-only behavior, literal arguments,
  manifest permission contracts, metadata IDs and temporary launcher-copy
  backup/idempotence/symlink handling. Shell syntax and AppStream validation pass.
- Cargo vendoring succeeds; 191 packages resolve offline with an empty Cargo
  cache from the prepared source snapshot. No builder or SDK was installed here.
- Full Flatpak build/sandbox acceptance is pending; CI is configured to build
  and upload a bundle. The local frontend's filtered-environment test is not
  equivalent to testing a real Flatpak sandbox.
