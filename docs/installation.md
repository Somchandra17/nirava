# Install Nirava on Linux

Nirava is a development preview. A source build in `target/` is not an installed
application. Installation is optional; none of these steps starts a service,
changes desktop configuration, or makes the backend writable.

Two distribution paths are provided:

| Method | Installs | Current scope |
|---|---|---|
| Homebrew on Linux | Native `nirava`, `niravactl`, `nirava-sessiond`, launcher/icon/metadata | Builds main with `--HEAD`; normal host discovery and optional reviewed actions |
| Flatpak | Sandboxed GTK frontend | Demo standalone; real session data via the native companion; host app launching and terminal package actions stay in the native build |

This is a project tap and a bundle recipe, not a Homebrew core or Flathub listing.
There is no macOS desktop support, stable release tag, signed release repository,
or prebuilt Homebrew bottle yet. Homebrew source installation is also exercised by its disposable CI workflow;
real local installation and Flatpak sandbox acceptance remain separate steps.

## Homebrew

Requires an existing Homebrew installation on Linux (version 7 or newer for the
formula's dependency fetch hook). The script does not bootstrap Homebrew. Review:

```sh
bash tools/install.sh brew
```

To install after reviewing the plan:

```sh
bash tools/install.sh brew --apply
```

This runs:

```sh
brew tap --custom-remote somchandra17/nirava https://github.com/Somchandra17/nirava.git
brew install --HEAD somchandra17/nirava/nirava
```

The formula declares Rust and pkgconf as build dependencies, plus GTK4,
libadwaita and PulseAudio libraries. Homebrew resolves their transitive
libraries too. Cargo dependencies are fetched first; compilation is locked and
offline. No PulseAudio daemon or Nirava service is started by this formula.

Run `nirava --demo` to preview, or `nirava` for read-only session discovery.
Homebrew manages the installed binaries and shared data. It does not change
your login shell or session's XDG_DATA_DIRS. If the desktop menu does not include
Homebrew's share directory, optionally review and copy the three launcher assets:

```sh
python3 tools/register-launcher.py --prefix "$(brew --prefix nirava)"
python3 tools/register-launcher.py --prefix "$(brew --prefix nirava)" --apply
```

The helper copies a launcher with an absolute executable path, icon and metadata
to `$XDG_DATA_HOME` (default `~/.local/share`). It refuses symlink destinations,
backs up existing files with timestamped names, and leaves identical files alone.
It copies files rather than linking your source checkout. It never registers a
service. On uninstall, these optional user copies remain: remove only the
`io.github.Somchandra17.Nirava.desktop`, `.svg`, and `.metainfo.xml` files printed
by the helper if no other Nirava installation uses them; keep any backups needed.

Upgrade/rebuild the current development branch with:

```sh
brew upgrade --fetch-HEAD somchandra17/nirava/nirava
```

Remove the native package with `brew uninstall nirava`. If you manually started
the companion, stop it in its terminal first. The example systemd unit is stored
as documentation under the package share directory; its `/usr/bin` path must be
tailored before any separately authorized deployment. There is no default
`brew services start` step.

## Build a Flatpak bundle

Full build prerequisites are `cargo`, `flatpak-builder`, Flatpak, GNOME SDK 51,
GNOME Platform 51, and the Freedesktop Rust SDK extension for 26.08. The SDK can
be large. Install those prerequisites only when you choose to do so. If Flathub
is already configured, the explicit runtime/SDK preparation command is:

```sh
flatpak install --user flathub org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08
```

No script adds a remote or installs a runtime automatically. Cargo source
preparation can be checked separately without `flatpak-builder`:

```sh
python3 tools/build-flatpak.py --prepare-only
```

Build the bundle once the prerequisites are available:

```sh
python3 tools/build-flatpak.py
```

The helper stages only the workspace source, data, lockfile and license in a new
`target/flatpak-build-*` directory. Cargo vendors checksummed dependencies there.
Flatpak's compilation runs offline without network permission. No host filesystem
access, host execution bridge, whole-bus permission, or privileged daemon is
included in the application. The manifest grants Wayland, rendering devices and
communication with Nirava's one companion bus name. GNOME 51 uses the
Freedesktop 26.08 Rust extension. Build logs and caches stay under `target/`.

The output is `target/nirava.flatpak`; existing bundles are not overwritten.
Use `--output /another/path/nirava.flatpak` for another build. This command
builds a bundle without installing it. The `flatpak` GitHub Actions workflow
builds the same recipe in a disposable runner and uploads a bundle artifact;
it is not a published release or a Flathub submission.

## Install and use the Flatpak frontend

Review the user installation command, then opt in to applying it:

```sh
bash tools/install.sh flatpak --bundle target/nirava.flatpak
bash tools/install.sh flatpak --bundle target/nirava.flatpak --apply
flatpak run io.github.Somchandra17.Nirava --demo
```

Flatpak presents its own installation and permission review. The target runtime
must be available through your configured remotes/installed runtimes. This
script does not configure repositories for you.

For actual host data, start the **native** companion in a terminal in your
current graphical login, using the native Homebrew install or your native source
build. It stays read-only:

```sh
nirava-sessiond
# Or: ./target/debug/nirava-sessiond
```

Then launch the frontend from another terminal:

```sh
flatpak run --env=XDG_SESSION_ID="$XDG_SESSION_ID" io.github.Somchandra17.Nirava
```

The frontend checks that the native companion's nonempty login-session ID
matches its own. Native frontend checks still include compositor identity.
If the companion is missing or identity cannot be verified, the frontend shows
an unavailable state rather than inspecting the sandbox as if it were the host.

The Flatpak frontend can display host inventory and use already gated companion
operations. It cannot launch arbitrary host desktop apps or terminal package
commands; use the native build for those. `--enable-package-actions` does not
bypass that limit. Without a companion, use demo mode to inspect the interface.
No sandbox escape via `flatpak-spawn --host` is shipped.

Remove the frontend with:

```sh
flatpak uninstall --user io.github.Somchandra17.Nirava
```

No `--delete-data` or automatic runtime cleanup is requested. The separately
installed native companion is managed independently by Homebrew/source checkout.
