# Nirava engineering rules

Read docs/implementation-plan.md and README.md before changing behavior.
This app edits a desktop session: recovery and accurate state are release gates.
Default execution is read-only. Never use the user's live configuration for tests.
Use temporary directories, mock audio, and fake compositor/logind responses.
Do not install packages, install/enable services, suspend, reboot, log out, change
drivers, or deploy integration files without the user's explicit authorization.
Stage and validate changes, recheck source hashes, and make timestamped backups.
Never overwrite a concurrent user edit during commit or rollback.
No arbitrary shell RPC. Commands use argument vectors and bounded timeouts.
Do not claim a milestone complete unless its documented acceptance checks pass.
