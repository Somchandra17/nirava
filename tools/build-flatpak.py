#!/usr/bin/env python3
"""Stage offline Cargo sources and build a bundle. Never installs or adds remotes."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "io.github.Somchandra17.Nirava"


def prepare(root, work):
    source = work / "source"
    source.mkdir()
    for name in ("Cargo.toml", "Cargo.lock", "LICENSE"):
        shutil.copy2(root / name, source / name)
    for name in ("crates", "data"):
        shutil.copytree(root / name, source / name, symlinks=True)
    # Reject links: directory sources must be a self-contained source snapshot.
    if any(path.is_symlink() for path in source.rglob("*")):
        raise RuntimeError("Source snapshot contains a symlink; refusing to package it")
    with (work / "vendor.log").open("w") as log:
        result = subprocess.run(
            ["cargo", "vendor", "--locked", "--versioned-dirs", str(source / "vendor")],
            cwd=root, text=True, stdout=subprocess.PIPE, stderr=log, check=True,
        )
    config = result.stdout
    (source / ".cargo").mkdir()
    (source / ".cargo/config.toml").write_text(config.replace(str(source / "vendor"), "vendor"))
    manifest = root / "packaging/flatpak" / (APP_ID + ".json")
    shutil.copy2(manifest, work / manifest.name)
    return work / manifest.name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare-only", action="store_true", help="Vendor/stage sources without flatpak-builder")
    parser.add_argument("--output", type=Path, help="Bundle destination; refuses to overwrite")
    args = parser.parse_args()
    if os.uname().sysname != "Linux":
        parser.error("Nirava Flatpak builds require Linux")
    for command in ("cargo",) if args.prepare_only else ("cargo", "flatpak-builder", "flatpak"):
        if shutil.which(command) is None:
            parser.error(f"Missing {command}; install build prerequisites explicitly, then retry")
    output = (args.output or ROOT / "target/nirava.flatpak").absolute()
    if output.exists() or output.is_symlink():
        parser.error(f"Refusing to overwrite {output}")
    (ROOT / "target").mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="flatpak-build-", dir=ROOT / "target"))
    manifest = prepare(ROOT, work)
    print(f"Prepared offline source snapshot: {work}", flush=True)
    if args.prepare_only:
        return
    # Prerequisite runtimes must already be installed. Never auto-install them.
    subprocess.run(["flatpak-builder", "--user", "--disable-rofiles-fuse", "--repo=" + str(work / "repo"),
                    str(work / "build"), str(manifest)], cwd=work, check=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["flatpak", "build-bundle", str(work / "repo"), str(output), APP_ID], check=True)
    print(f"Built {output}. This bundle has not been installed.")


if __name__ == "__main__":
    main()
