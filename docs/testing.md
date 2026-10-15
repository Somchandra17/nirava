# Testing and acceptance

## Automated checks

```sh
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 tests/dbus_smoke.py
```

`rustfmt` and `clippy` must be installed to run the first/third checks; they
are absent on the current machine and have not been run here.

The D-Bus check uses an isolated `dbus-run-session`, the demo service, and
temporary XDG paths. It tests real IPC while refusing writes. File transaction
tests use disposable fixtures; compositor recovery tests use fake drivers and
a virtual clock. They never call live display mutation, logind suspend, or
change a desktop config.

With native build dependencies available:

```sh
cargo build --locked --workspace --features plain-service/audio,plain-cli/audio
cargo clippy --locked --workspace --all-targets --features plain-service/audio,plain-cli/audio -- -D warnings
cargo run -p plain-ui -- --demo --smoke-test
```

The GUI smoke test briefly presents a fictional-device window and exits.
For the tested Fedora runtime-only fallback, build with
`bash tools/build-with-runtime-libs.sh`, then run
`python3 tests/frontend_smoke.py` on an existing graphical display. This tests
the real GTK client against the demo backend on a bus with helper activation
disabled. A GUI/daemon namespace collision makes this test fail.
Inspect all four pages at wide and narrow sizes, keyboard-only navigation,
screen-reader labels, text scaling, and light/dark themes. Check read-only
controls remain disabled and unsupported operations have accurate reasons.

## Human-operated acceptance (not automated)

Do these only after approving the exact integration changes and backups.
Keep an alternate terminal/recovery login available when testing display modes.

1. Compare read-only audio inventory with `wpctl status`. Exercise a headset
   default, new playback stream, mic default, mute, and volume; restore them.
2. Review Noctalia changes; verify comments/custom behaviors survive, file
   reload is effective, and Restore recovers the original. Confirm caffeine
   stops the existing owner's idle actions and releases on backend exit.
3. Test lock readiness and Suspend manually. Confirm the lock is secure before
   sleep and after resume. Agent-run suspend, reboot, and logout are excluded.
4. Test supervised display preview: decline confirmation, close the window,
   kill/restart the backend, hotplug/unplug, and try an invalid advertised mode.
   Every pending preview must recover to a usable connected layout.
5. Once persistence is implemented, inspect exact config diff, externally edit
   a staged target to force refusal, simulate reload failure, exercise Restore,
   then verify a normal relogin retains the chosen configuration.
6. Repeat with the Hyprland laptop and its actual hypridle/kanshi setup.

Do not mark v1 complete on the strength of fixture tests alone.
