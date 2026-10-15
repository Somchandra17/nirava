#!/usr/bin/env python3
"""Copy a native install's launcher/icon into your user data directory, with backups."""
import argparse
from datetime import datetime
import os
from pathlib import Path
import shutil
import tempfile

APP_ID = "io.github.Somchandra17.Nirava"


def safe_destination(path):
    if path.exists() and not path.is_file():
        raise ValueError(f"Refusing non-file destination: {path}")
    current = path
    while True:
        if current.is_symlink():
            raise ValueError(f"Refusing symlink destination/ancestor: {current}")
        if current.parent == current:
            break
        current = current.parent


def launcher_files(prefix):
    prefix = Path(prefix).absolute()
    executable = str(prefix / "bin/nirava")
    if any(char in executable for char in "\n\r\x00"):
        raise ValueError("Prefix contains a control character")
    quoted = '"' + executable.replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$") + '"'
    result = {}
    for name in [f"applications/{APP_ID}.desktop", f"icons/hicolor/scalable/apps/{APP_ID}.svg", f"metainfo/{APP_ID}.metainfo.xml"]:
        source = prefix / "share" / name
        data = source.read_bytes()
        if name.endswith(".desktop"):
            data = data.decode().replace("Exec=nirava\n", f"Exec={quoted}\n").encode()
        result[name] = data
    return result


def register(prefix, data_home, apply=False):
    files = launcher_files(prefix)
    # Validate every destination before any writes.
    for name in files:
        safe_destination(data_home / name)
    for name, content in files.items():
        target = data_home / name
        print(f"Copy {target}")
        if not apply:
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        safe_destination(target)
        if target.exists():
            if not target.is_file():
                raise ValueError(f"Refusing non-file destination: {target}")
            if target.read_bytes() == content:
                continue
            backup = target.with_name(target.name + ".bak-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f"))
            shutil.copy2(target, backup)
            print(f"Backup {backup}")
        with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as staged:
            staged_path = Path(staged.name)
            try:
                staged.write(content)
                staged.flush()
                os.fsync(staged.fileno())
                staged_path.chmod(0o644)
                safe_destination(target)
                staged_path.replace(target)
            finally:
                staged_path.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    if os.uname().sysname != "Linux":
        parser.error("Nirava supports Linux only")
    if args.apply and os.geteuid() == 0:
        parser.error("Run as your normal desktop user")
    data_home = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))).absolute()
    register(args.prefix, data_home, args.apply)
    if not args.apply:
        print("Plan only. Add --apply to copy these files; existing files receive timestamped backups.")


if __name__ == "__main__":
    main()
