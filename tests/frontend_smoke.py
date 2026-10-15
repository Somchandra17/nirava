#!/usr/bin/env python3
"""Run the real GTK client with the demo backend on a private user bus.

Requires an existing graphical display. Captures only Nirava's rendered scene.
It does not change settings, connect to the real session backend, or use audio.
"""
import json
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def inside():
    candidates = [ROOT / "target/debug/nirava", ROOT / "target/runtime-build/debug/nirava"]
    ui = next((path for path in candidates if path.is_file()), None)
    if ui is None:
        raise SystemExit("Build the native window first")
    env = os.environ.copy()
    env["GSK_RENDERER"] = "cairo"
    env.update(GTK_USE_PORTAL="0", GIO_USE_VFS="local", NO_AT_BRIDGE="1", GSETTINGS_BACKEND="memory")
    proc = subprocess.Popen([str(ROOT / "target/debug/nirava-sessiond"), "--demo"],
                            env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    try:
        for _ in range(50):
            result = subprocess.run(["gdbus", "call", "--session", "--dest",
                                    "io.github.Somchandra17.Nirava.Session", "--object-path",
                                    "/io/github/Somchandra17/Nirava", "--method",
                                    "io.github.Somchandra17.Nirava.Manager1.GetSessionKey"],
                                    env=env, capture_output=True, text=True, timeout=15)
            if result.returncode == 0:
                break
            if proc.poll() is not None:
                raise AssertionError(proc.stderr.read())
            time.sleep(0.1)
        else:
            raise AssertionError("Demo backend did not start")
        with tempfile.TemporaryDirectory(prefix="nirava-frontend-") as temp:
            capture = Path(temp) / "window.png"
            # No --demo flag: this must read the private-bus demo backend.
            result = subprocess.run([str(ui), "--page", "sound", "--smoke-test",
                                     "--test-width", "540", "--snapshot", str(capture)],
                                    env=env, capture_output=True, text=True, timeout=20)
            assert result.returncode == 0, result.stderr
            assert capture.is_file() and capture.stat().st_size > 1000, result.stderr
            assert proc.poll() is None
            result = subprocess.run([str(ROOT / "target/debug/niravactl"), "status", "--json"],
                                    env=env, capture_output=True, text=True, timeout=15)
            assert json.loads(result.stdout)["read_only"] is True
    finally:
        proc.terminate()
        proc.wait(timeout=5)
        proc.stderr.close()
    assert proc.returncode == 0
    print("PASS: native frontend and session backend coexist and exchange read-only snapshots")


if __name__ == "__main__":
    if "--inside" in sys.argv:
        inside()
    else:
        result = subprocess.run(["dbus-run-session", "--config-file", str(ROOT / "tests/private-bus.conf"), "--", sys.executable,
                                 str(Path(__file__).resolve()), "--inside"], timeout=90)
        raise SystemExit(result.returncode)
