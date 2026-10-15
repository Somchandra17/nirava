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
