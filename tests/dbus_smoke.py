#!/usr/bin/env python3
"""Real IPC on a private session bus, with demo data and temporary XDG paths."""
import json
import ast
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def inside():
    service = ROOT / "target/debug/nirava-sessiond"
    cli = ROOT / "target/debug/niravactl"
    if not service.is_file() or not cli.is_file():
        raise SystemExit("Run cargo build --locked before this check")
    with tempfile.TemporaryDirectory(prefix="nirava-ipc-test-") as temp:
        env = os.environ.copy()
        for name in ("NIRI_SOCKET", "HYPRLAND_INSTANCE_SIGNATURE", "WAYLAND_DISPLAY"):
            env.pop(name, None)
        env.update(HOME=temp, XDG_CONFIG_HOME=temp + "/config",
                   XDG_STATE_HOME=temp + "/state", XDG_RUNTIME_DIR=temp + "/runtime",
                   XDG_SESSION_ID="nirava-test")
        Path(env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)

        def run(*args):
            return subprocess.run([str(cli), *args], env=env, text=True,
                                  capture_output=True, timeout=15)

        for flags in ([], ["--allow-writes"]):
            proc = subprocess.Popen([str(service), "--demo", *flags], env=env,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                    text=True)
            try:
                for _ in range(50):
                    if proc.poll() is not None:
                        raise AssertionError(proc.stderr.read())
                    ready = subprocess.run(["gdbus", "call", "--session", "--dest",
                                            "io.github.Somchandra17.Nirava.Session", "--object-path",
                                            "/io/github/Somchandra17/Nirava", "--method",
                                            "io.github.Somchandra17.Nirava.Manager1.GetSessionKey"],
                                           env=env, text=True, capture_output=True, timeout=5)
                    if ready.returncode == 0:
                        status = run("status", "--json")
                        state = json.loads(status.stdout)
                        if state["session"]["compositor"] == "Niri (simulation)":
                            break
                    time.sleep(0.1)
                else:
                    raise AssertionError("Demo backend did not answer on the private bus")
                assert state["read_only"] is True
                assert state["schema_version"] == 1
                assert state["sound"]["default_output"] == "demo-headset"
                requests = [("stage-power", "--screen-off", "60", "--sleep", "120"),
                            ("commit", "power-1", "--yes"),
                            ("restore", "power-1", "--yes"),
                            ("caffeine", "on"), ("suspend",)]
                for args in requests:
                    result = run(*args)
                    assert result.returncode != 0, (args, result.stdout)
                    assert "read-only" in result.stdout + result.stderr, (args, result)
                for method, args in (("SetVolume", ["1", "999", "false", "false", "0.1"]),
                                     ("SetMute", ["1", "999", "false", "false", "true"]),
                                     ("SetDefaultDevice", ["1", "fake-device", "false"])):
                    result = subprocess.run(["gdbus", "call", "--session", "--dest",
                                             "io.github.Somchandra17.Nirava.Session",
                                             "--object-path", "/io/github/Somchandra17/Nirava",
                                             "--method", "io.github.Somchandra17.Nirava.Manager1." + method,
                                             *args], env=env, text=True, capture_output=True, timeout=10)
                    assert result.returncode == 0, result.stderr
                    operation = json.loads(ast.literal_eval(result.stdout)[0])
                    assert operation["status"] == "failed"
                    assert "read-only" in operation["message"]
                assert not Path(env["XDG_CONFIG_HOME"]).exists()
                assert not Path(env["XDG_STATE_HOME"]).exists()
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
                proc.stderr.close()
            assert proc.returncode == 0, "SIGTERM should shut the backend down cleanly"
    print("PASS: private-bus snapshots, config/audio write refusal, and clean SIGTERM shutdown")


if __name__ == "__main__":
    if "--inside" in sys.argv:
        inside()
    else:
        result = subprocess.run(["dbus-run-session", "--config-file", str(ROOT / "tests/private-bus.conf"), "--", sys.executable,
                                 str(Path(__file__).resolve()), "--inside"], timeout=90)
        raise SystemExit(result.returncode)
