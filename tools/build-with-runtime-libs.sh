#!/usr/bin/env bash
# Local fallback for a Fedora development machine that already has the native
# runtime libraries but lacks pkg-config development files. No installation.
# Generated Rust FFI needs no C compilation here. Explicitly link the existing
# runtime SONAMEs; use a separate build directory so normal builds stay clean.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 - <<'PY'
import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Gtk, Adw
assert (Gtk.get_major_version(), Gtk.get_minor_version()) >= (4, 12)
assert (Adw.get_major_version(), Adw.get_minor_version()) >= (1, 6)
PY
libraries=(libgtk-4.so.1 libadwaita-1.so.0 libglib-2.0.so.0
           libgobject-2.0.so.0 libgio-2.0.so.0 libgdk_pixbuf-2.0.so.0
           libcairo.so.2 libcairo-gobject.so.2 libpango-1.0.so.0
           libpangocairo-1.0.so.0 libgraphene-1.0.so.0 libpulse.so.0)
link_args=()
for library in "${libraries[@]}"; do
    link_args+=(-C "link-arg=-Wl,-l:${library}")
done
# libpulse-sys needs its normal SONAME alias metadata, rather than its docs
# build. Only remove that crate's artifacts in this private target directory.
CARGO_TARGET_DIR=target/runtime-build cargo clean -p libpulse-sys
CARGO_TARGET_DIR=target/runtime-build cargo build --locked -p plain-backends --features audio
CARGO_TARGET_DIR=target/runtime-build DOCS_RS=1 cargo rustc --locked -p plain-ui --bin plain -- "${link_args[@]}"
