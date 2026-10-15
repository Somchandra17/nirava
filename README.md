# Nirava

[![Build](https://github.com/Somchandra17/nirava/actions/workflows/build.yml/badge.svg)](https://github.com/Somchandra17/nirava/actions/workflows/build.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

**Tune your Wayland desktop.**

A native Linux desktop control center for **Niri** and **Hyprland** on **Wayland**,
built with **Rust**, **GTK 4**, and **libadwaita**.
Manage sound through PipeWire/PulseAudio, preview display layouts, and review
power settings alongside Noctalia and existing session tools.
Rust workspace: `nirava` (window), `nirava-sessiond` (session D-Bus backend),
and `niravactl` (inspection and reviewed operations).

**Development preview, not a finished v1.** Default execution is read-only.
Nothing in this repository installs itself, replaces an idle engine, enables a
service, or modifies the desktop at build time.

## Build and inspect

Rust 1.95 or newer is required. The CLI and backend build without GTK headers:

```sh
cargo build --locked
cargo test --locked
cargo run -p nirava-cli -- doctor
cargo run -p nirava-cli -- status --demo --json
```

The native window additionally needs GTK >= 4.12 and libadwaita >= 1.6
development files. On Fedora these are `gtk4-devel` and `libadwaita-devel`.
`pulseaudio-libs-devel` provides normal audio build discovery, although the audio
crate can link the installed `libpulse.so.0` without it. Installing packages
is a separate, explicit user decision.

```sh
cargo build --locked --workspace --features nirava-service/audio,nirava-cli/audio
cargo run -p nirava-ui -- --demo
cargo run -p nirava-ui -- --page system
```

On this Fedora machine the preview also builds without installing packages:

```sh
bash tools/build-with-runtime-libs.sh
./target/runtime-build/debug/nirava --demo
./target/runtime-build/debug/nirava --page system
```

This fallback links the already installed runtime libraries directly and
checks their minimum versions. Normal development/CI builds use pkg-config
and the development packages. The final command reads your real session;
controls remain read-only unless a writable backend was explicitly started.

`--demo` uses fictional devices and refuses writes. With no running backend,
the window performs local read-only discovery. Start the read-only backend
manually with `cargo run -p nirava-service --features audio`; the window connects
over the user D-Bus. Clients verify session identity. Run both programs as your
normal desktop user.

## Current implementation

| Component | Implemented | Remaining release checks |
|---|---|---|
| Window | Native build and smoke tests; responsive Adwaita sidebar, search, four pages; wide/narrow demo screenshots inspected | Keyboard/screen-reader acceptance and launcher/laptop checks |
| Discovery | Real Niri/Hyprland IPC probes, output inventory, idle/display owners, logind capabilities/inhibitors | Laptop validation and broader compositor support |
| Audio | Native libpulse worker builds; real PipeWire inventory, subscriptions, defaults, volume/mute, playback streams, stale-server refusal and write readback | Human-operated device mutation tests |
| Noctalia power | TOML editing preserving numeric types/comments, review/commit, backups and Restore in System, caffeine inhibitor, gated manual Suspend | Effective-engine confirmation and human suspend acceptance |
| Displays | Editor, timed confirmation, Niri validation, supervised 15-second preview, journal recovery, hotplug-aware revert; pure KDL writer | Persistent Save integration, real supervised crash tests, laptop/kanshi support |
| Safety | Read-only default, temporary-file/fake-driver tests, guarded restore | Whole-process crash and real hardware acceptance |

`nirava-sessiond --allow-writes` is an explicit opt-in for the implemented
operations. Do not enable it as a daily service before the acceptance checks.
Suspend has a further `--enable-suspend` manual-test gate. The window exposes
the button only in that explicitly enabled mode with an enabled Noctalia
lockscreen and logind permission. No automated test suspends the machine.

Display preview additionally requires `--enable-display-preview` and real
supervision by `nirava-sessiond.service`. The supplied unit is read-only by
default. Keeping a preview currently keeps it **only for the current login**;
it does not save compositor files. Persistent Save remains unavailable.

Noctalia remains the desktop idle owner. Hypridle, its existing locker, and
kanshi remain the laptop owners. Nirava does not run a second idle engine.

## Project guide

- [Implementation plan](docs/implementation-plan.md): architecture, remaining milestones, release gates.
- [Testing](docs/testing.md): safe automated checks and manual acceptance.
- [D-Bus API](docs/dbus.md): versioned interface and operation semantics.
- [Verification record](docs/verification.md): checks actually run and unresolved gates.
- `data/`: window template, icon, desktop entry, and user-unit definitions.

GPL-3.0-or-later. The implementation plan records the selected existing-engine
approach and supersedes the original vision’s separate idle daemon.
