# Code review — 2026-10-15

Reviewed the Rust workspace (core, backends, service, CLI and GTK frontend),
new Apps implementation, build/CI scripts, integration definitions and tests.
This is a source-level engineering review with safe local validation, not a
security certification or evidence that pending hardware acceptance has passed.

## Resolved issues

- **Stale power staging:** a file reread could associate a newer original hash
  with a proposed edit based on older contents. Staging now checks the exact
  source text used by the editor. Configuration commit/restore RPCs serialize
  through one lock. Disposable-file regressions cover intervening edits.
- **Path redirection and journal parsing:** configuration targets, existing
  ancestors and restore backups reject symlinks/nonregular files. Backup files
  start private. Restore journal reads use the same bounded path checks as
  backup listing. A read/rename race with external writers still remains below.
- **Malformed configuration:** invalid enabled flags and malformed idle tables
  return errors; duplicate/aliased output requests, invalid scales and unknown
  transforms are refused before span editing. Unknown transforms no longer
  silently become normal orientation for newly created output blocks.
- **Display recovery:** the watchdog retries temporary recovery failures while
  retaining the journal. Recovery attempts have time bounds. Keep checks the
  deadline again after compositor readback. Journal creation/deletion syncs the
  parent directory. Layout requests cannot replace advertised capabilities or
  supply positions beyond the UI's supported range. Tests use fake drivers.
- **Audio state:** enumeration errors are propagated instead of treated as empty
  successful lists. List collection has total time/size bounds. Device/stream
  creation or removal invalidates target tokens even within one server session;
  ordinary volume events do not. Expired queued writes are rejected before
  execution and checked again after inventory reads. A response timeout reports
  an unknown outcome, since an operation already started may still complete.
- **Frontend state:** permission and audio-token changes force control refresh
  even when a widget has focus. Apps filter/remote callbacks use weak widget
  references, avoiding signal reference cycles on repeated page refreshes. Software filters
  and search text survive inventory refreshes.
- **Packages:** validate exact identifiers, preserve Flatpak scope/arch/branch,
  accept real Flatpak column formats and hyphenated application IDs, target the
  selected RPM version for removal, and disable unused-dependency cleanup.
  Plans reject file paths, RPM downloads, wildcard/multiple targets, options and
  shell syntax. Existing remotes only; no automatic yes or data deletion. Actual
  dependency review and final consent belong to interactive DNF/Flatpak.
- **Tooling:** the Fedora runtime build selects system Python rather than an
  unrelated PATH interpreter without GI. CI has a time limit and read-only
  repository permissions. Isolated GTK tests explicitly disable accessibility
  service lookup only within their private test environment.

## Validation

See [verification](verification.md) for the actual checks and limitations.
All automated writes stay in temporary fixtures or ignored build directories.
The live machine was queried for installed-package inventory only. No app was
launched, terminal package operation started, service installed/enabled,
configuration deployed, display changed, audio changed, or power action invoked.

## Remaining release gates

- Filesystem hash checks and ancestor checks do not provide an atomic
  compare-and-swap with an uncooperative external editor. Directory-descriptor
  path handling, journal-directory creation and multi-file crash consistency
  require further hardening before persistence/setup is released.
- Audio mutation tokens reduce stale-device races but cannot make independent
  server-side changes atomic with a client request. Real device mutation and
  server restart acceptance remain necessary.
- Real supervised display crash/hotplug tests, secure lock/suspend acceptance,
  Noctalia reload/Restore, laptop adapters and accessibility acceptance remain
  pending. Persistent display Save is still unavailable.
- Package transactions currently use a reviewed terminal handoff. Native
  progress/dependency UI, AppStream catalog, permissions editor, custom Flatpak
  installations and other native package managers are not implemented.
- No rustfmt/clippy tools were available locally; no packages were installed to
  add them. CI performs the standard native build and fixture tests.
