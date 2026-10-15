# Nirava

[![Build](https://github.com/Somchandra17/nirava/actions/workflows/build.yml/badge.svg)](https://github.com/Somchandra17/nirava/actions/workflows/build.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

**Tune your Wayland desktop.**

A native Linux desktop control center for **Niri** and **Hyprland** on **Wayland**,
built with **Rust**, **GTK 4**, and **libadwaita**.
Manage sound through PipeWire/PulseAudio, preview display layouts, and review
power settings alongside Noctalia and existing session tools. Browse installed
applications, Fedora RPM packages, and user/system Flatpak apps.
Rust workspace: `nirava` (window), `nirava-sessiond` (session D-Bus backend),
and `niravactl` (inspection and reviewed operations).

**Development preview, not a finished v1.** Default execution is read-only.
Nothing in this repository installs itself, replaces an idle engine, enables a
service, or modifies the desktop at build time.

## Installation

See [the installation guide](docs/installation.md) for the Linux Homebrew formula,
Flatpak frontend/bundle builder, prerequisites and uninstall steps. The installer
prints a plan by default:

```sh
bash tools/install.sh brew
bash tools/install.sh flatpak --bundle target/nirava.flatpak
```

Add `--apply` only when you want to install. Homebrew provides the native app and
companion; Flatpak uses the companion for host discovery. No service is started.
These are development packaging paths, not a Flathub/Homebrew core release.

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

The helper uses Fedora’s `/usr/bin/python3` for GI; `NIRAVA_PYTHON` can select
another interpreter with the GTK/Adwaita bindings.

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
| Window | Native build and smoke tests; responsive Adwaita sidebar, search, five pages; wide/narrow demo screenshots inspected | Keyboard/screen-reader acceptance and launcher/laptop checks |
| Discovery | Real Niri/Hyprland IPC probes, output inventory, idle/display owners, logind capabilities/inhibitors | Laptop validation and broader compositor support |
| Audio | Native libpulse worker builds; real PipeWire inventory, subscriptions, defaults, volume/mute, playback streams, stale-server refusal and write readback | Human-operated device mutation tests |
| Noctalia power | TOML editing preserving numeric types/comments, review/commit, backups and Restore in System, caffeine inhibitor, gated manual Suspend | Effective-engine confirmation and human suspend acceptance |
| Displays | Editor, timed confirmation, Niri validation, supervised 15-second preview, journal recovery, hotplug-aware revert; pure KDL writer | Persistent Save integration, real supervised crash tests, laptop/kanshi support |
| Apps | Search/open desktop apps; local RPM and scoped Flatpak inventory; reviewed install/update/remove commands and optional interactive terminal handoff | Human transaction acceptance, available-software catalog, other native package managers |
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

## Apps and package management

Open Apps with `nirava --page apps`. The application list follows the desktop's
GIO application registry, including visibility and user overrides. Package views
show locally installed RPM packages and Flatpak apps, with user/system scope,
version, architecture, and Flatpak remote. Search narrows the list; at most 100
matching rows are displayed. Refresh rereads local inventory without refreshing
repository metadata or starting package transactions.

Review install, update, or removal commands in the package view. Flatpak installs
use a selected existing remote; RPM installs take an exact package name. In the
default read-only mode, you can copy commands. To enable terminal handoff:

```sh
./target/runtime-build/debug/nirava --page apps --enable-package-actions
```

**Continue in Terminal** opens DNF or Flatpak, which resolves dependencies,
requests authentication when necessary, and asks for final confirmation. Review
the complete dependency/removal list there. Nirava does not automatically approve
transactions, add repositories, delete Flatpak app data, force operations, or run
a root daemon. RPM removal disables automatic cleanup of unused dependencies;
other required dependent removals can still be proposed by DNF. Removal targets
the selected installed RPM version. Flatpak targets retain scope, architecture,
and branch. DNF actions are disabled on OSTree systems.

The terminal owns progress, cancellation and errors. Closing it does not guarantee
rollback; let active transactions finish, then use Refresh to check inventory.
Nirava does not claim success when a terminal merely opens. Supported terminals
are Kitty, foot, GNOME Terminal, Konsole, and xterm. Demo mode disables app launches
and terminal handoff. No package changes are made by automated tests.

CLI inspection and command review (the latter prints only):

```sh
niravactl apps --json
niravactl apps --demo --json
niravactl package-plan rpm install vlc
niravactl package-plan flatpak-user install org.gnome.Calculator --remote flathub
```

This is an installed-software manager with interactive transactions. An online
app catalog, AppStream descriptions/screenshots, a native transaction-progress UI,
Flatpak permission editing, custom Flatpak installations, AppImage integration,
and other native package managers remain future work.

## Project guide

- [Implementation plan](docs/implementation-plan.md): architecture, remaining milestones, release gates.
- [Installation](docs/installation.md): Homebrew, Flatpak, launcher registration and removal.
- [Testing](docs/testing.md): safe automated checks and manual acceptance.
- [D-Bus API](docs/dbus.md): versioned interface and operation semantics.
- [Verification record](docs/verification.md): checks actually run and unresolved gates.
- [Code review](docs/code-review.md): resolved issues and remaining engineering limits.
- `data/`: window template, icon, desktop entry, and user-unit definitions.

GPL-3.0-or-later. The implementation plan records the selected existing-engine
approach and supersedes the original vision’s separate idle daemon.
