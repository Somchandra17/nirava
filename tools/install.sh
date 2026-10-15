#!/usr/bin/env bash
# Read-only plan by default. --apply delegates to an interactive package manager.
# Does not bootstrap package managers, add Flatpak remotes/runtimes, or start services.
set -euo pipefail
usage() {
    cat <<'HELP'
Usage: bash tools/install.sh brew [--apply]
       bash tools/install.sh flatpak --bundle PATH [--apply]

Default: print the exact installation commands without changing anything.
brew: tap this project's repository and build/install its Linux-only HEAD formula.
flatpak: install an already built bundle into your user installation, interactively.
No service, shell setting, driver or desktop configuration is changed by this script.
HELP
}
method=""; bundle=""; apply=false
while (($#)); do
    case "$1" in
        brew|flatpak) [[ -z "$method" ]] || { usage >&2; exit 2; }; method="$1"; shift ;;
        --bundle) (($#>=2)) || { usage >&2; exit 2; }; bundle="$2"; shift 2 ;;
        --apply) apply=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
[[ "$(uname -s)" == Linux ]] || { echo "Nirava currently supports Linux/Wayland only." >&2; exit 1; }
[[ -n "$method" ]] || { usage >&2; exit 2; }
if $apply && ((EUID==0)); then echo "Run as your normal desktop user, without sudo." >&2; exit 1; fi
run() {
    printf '  '; printf '%q ' "$@"; printf '\n'
    if $apply; then "$@"; fi
}
case "$method" in
    brew)
        [[ -z "$bundle" ]] || { echo "--bundle applies only to Flatpak." >&2; exit 2; }
        if $apply; then command -v brew >/dev/null || { echo "Homebrew is missing; no bootstrap was run." >&2; exit 1; }; fi
        echo "Homebrew plan: source build of Nirava and its GTK/libadwaita/PulseAudio dependencies."
        run env HOMEBREW_NO_AUTO_UPDATE=1 brew tap --custom-remote somchandra17/nirava https://github.com/Somchandra17/nirava.git
        run env HOMEBREW_NO_AUTO_UPDATE=1 brew install --HEAD somchandra17/nirava/nirava
        echo "Then run nirava --demo. Start nirava-sessiond manually for the Flatpak frontend."
        ;;
    flatpak)
        [[ -n "$bundle" ]] || { echo "Pass --bundle PATH to an already built Nirava .flatpak bundle." >&2; exit 2; }
        [[ "$bundle" == *.flatpak ]] || { echo "Expected a .flatpak bundle." >&2; exit 2; }
        if $apply; then
            command -v flatpak >/dev/null || { echo "Flatpak is missing; no packages were installed." >&2; exit 1; }
            [[ -f "$bundle" && ! -L "$bundle" ]] || { echo "Bundle is missing or a symlink." >&2; exit 1; }
        fi
        echo "Flatpak plan: install the supplied bundle for your user; review its permissions at the prompt."
        run flatpak install --user --bundle -- "$bundle"
        echo "Then run: flatpak run io.github.Somchandra17.Nirava --demo"
        ;;
esac
if ! $apply; then echo "Plan only. Add --apply to perform these operations after review."; fi
