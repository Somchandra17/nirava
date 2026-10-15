# Session D-Bus API

Bus name: `io.github.Somchandra17.Nirava.Session` (user bus only).
Object: `/io/github/Somchandra17/Nirava`.
Interface: `io.github.Somchandra17.Nirava.Manager1`.

Snapshots are JSON strings with schema_version=1 and typed Rust domain models.
This early wire format is not a stable public API. Structured D-Bus records
and change signals may replace JSON before release.

| Method | Arguments | Result |
|---|---|---|
| GetSessionId / GetSessionKey | none | Session identifier / environment-derived session hash |
| GetSnapshot | none | JSON snapshot including read_only and capabilities |
| StagePower | IdleSettings JSON | Reviewed before/after, path, transaction ID |
| CommitChange | staged ID | OperationResult JSON |
| CancelChange | staged ID | none |
| RestoreChange | committed ID | Guarded backup restore OperationResult |
| SetCaffeine | enabled bool | OperationResult |
| SetVolume | generation u64, index u32, input bool, stream bool, volume f64 | OperationResult |
| SetMute | generation u64, index u32, input bool, stream bool, muted bool | OperationResult |
| SetDefaultDevice | generation u64, native name string, input bool | OperationResult |
| PreviewLayout | OutputState array JSON | 15-second Lease JSON |
| RevertLayout | lease ID | OperationResult |
| KeepLayoutForSession | lease ID | OperationResult; does not persist |
| Suspend | none | accepted/failed result, gated manual-test mode |

Read-only mode rejects every mutation before config/device work. Demo mode
rejects writes even if --allow-writes was supplied. Clients compare session
keys before operations. This is protection against accidental cross-session
use, not a security boundary against malicious processes running as the same
Unix user.

Sound snapshots contain a nonzero generation token. Pass that token back for
mutations; a reconnect invalidates old tokens before a recycled device index
can receive a stale request. Successful audio requests include state readback.

## Apps inventory

`GetApps() -> JSON` reads installed RPM packages and Flatpak applications on
demand. Fields include `loaded`, `simulation`, package-manager availability,
`immutable`, `packages`, configured `remotes` by scope, and per-source `errors`.
A failed source is reported rather than shown as an empty successful query.
`packages` retain source, exact app reference/package identity, version,
architecture and origin. RPM origin is the installed database, not an inferred
repository. Flatpak inventory covers user and default system installations.
Demo backends return fictional inventory even when --allow-writes is passed.

There is no package execution D-Bus method. Reviewed terminal handoff is a local
UI action requiring --enable-package-actions; package-manager consent remains
interactive. Package changes are not part of the config Restore mechanism.

## Flatpak frontend identity

The native frontend still verifies GetSessionKey against its compositor context.
The sandboxed frontend cannot see native compositor sockets, so it verifies
GetSessionId against a nonempty XDG_SESSION_ID in the same filtered user bus.
Missing/mismatched IDs refuse the companion. The Flatpak ships only the frontend
and grants access to this specific bus name, not the whole session/system bus.
No package executor or host-shell bridge is introduced.
