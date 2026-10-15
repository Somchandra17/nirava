#!/usr/bin/env python3
"""Packaging contracts and installer behavior; never installs software."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("launcher", ROOT / "tools/register-launcher.py")
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class PackagingTests(unittest.TestCase):
    def test_flatpak_has_no_host_escape_permissions(self):
        manifest = json.loads((ROOT / "packaging/flatpak/io.github.Somchandra17.Nirava.json").read_text())
        self.assertEqual(manifest["app-id"], launcher.APP_ID)
        self.assertEqual(set(manifest["finish-args"]), {"--socket=wayland", "--device=dri", "--talk-name=" + launcher.APP_ID + ".Session"})
        module = manifest["modules"][0]
        self.assertIn("--offline --locked", module["build-commands"][0])
        self.assertNotIn("sessiond", " ".join(module["build-commands"]))

    def test_metadata_matches_launcher(self):
        metadata = ET.parse(ROOT / "data" / (launcher.APP_ID + ".metainfo.xml")).getroot()
        self.assertEqual(metadata.findtext("id"), launcher.APP_ID)
        self.assertEqual(metadata.findtext("launchable"), launcher.APP_ID + ".desktop")

    def test_installer_defaults_to_plan_and_preserves_literal_arguments(self):
        # Fake package-manager executables would leave a marker if executed.
        with tempfile.TemporaryDirectory(prefix="nirava-install-test-") as temp:
            folder = Path(temp)
            marker = folder / "executed"
            for command in ("brew", "flatpak"):
                script = folder / command
                script.write_text("#!/bin/sh\n: > \"$NIRAVA_TEST_MARKER\"\n")
                script.chmod(0o755)
            env = dict(os.environ, PATH=str(folder) + ":" + os.environ["PATH"], NIRAVA_TEST_MARKER=str(marker))
            for args in [("brew",), ("flatpak", "--bundle", "target/a name;touch nope.flatpak")]:
                result = subprocess.run(["bash", str(ROOT / "tools/install.sh"), *args], env=env, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Plan only", result.stdout)
                self.assertFalse(marker.exists())
            invalid = subprocess.run(["bash", str(ROOT / "tools/install.sh"), "flatpak"], env=env, capture_output=True)
            self.assertNotEqual(invalid.returncode, 0)

    def test_launcher_plan_backup_and_repeat_install(self):
        with tempfile.TemporaryDirectory(prefix="nirava-launcher-test-") as temp:
            root = Path(temp)
            prefix, data = root / "prefix with spaces", root / "data"
            for name in [f"applications/{launcher.APP_ID}.desktop", f"icons/hicolor/scalable/apps/{launcher.APP_ID}.svg", f"metainfo/{launcher.APP_ID}.metainfo.xml"]:
                target = prefix / "share" / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / "data" / Path(name).name).read_bytes())
            launcher.register(prefix, data)
            self.assertFalse(data.exists())
            desktop = data / "applications" / (launcher.APP_ID + ".desktop")
            desktop.parent.mkdir(parents=True)
            desktop.write_text("user's original")
            launcher.register(prefix, data, apply=True)
            self.assertIn('Exec="' + str(prefix / "bin/nirava") + '"', desktop.read_text())
            backups = list(desktop.parent.glob("*.bak-*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_text(), "user's original")
            launcher.register(prefix, data, apply=True)
            self.assertEqual(len(list(desktop.parent.glob("*.bak-*"))), 1)
            actual = root / "elsewhere"
            actual.mkdir()
            linked = root / "linked"
            linked.symlink_to(actual, target_is_directory=True)
            with self.assertRaises(ValueError):
                launcher.register(prefix, linked, apply=True)
            self.assertEqual(list(actual.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
