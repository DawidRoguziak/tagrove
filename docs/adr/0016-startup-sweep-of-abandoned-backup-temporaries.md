# 0016: Startup removes abandoned backup temporaries by exact name, and a failed pre-swap restore removes its staging

- Status: Accepted
- Date: 2026-11-11
- Areas: data safety (bundle export, inspection, restore), startup order, profiles
- Related: [data safety](../subsystems/data-safety-and-portability.md#restore-rollback-and-recovery), [system overview](../architecture/system-overview.md#executable-bootstrap-and-runtime-lifecycle), [ADR 0015](0015-restore-tolerates-unresolved-thumbnails-and-nullable-file-name.md)

## Context

Bundle work writes full database copies into the profile directory (`src-tauri/src/services/backup_service.rs`):

- export writes its SQLite Backup API snapshot to `backup-snapshot-<pid>-<n>.db`;
- inspection extracts the archive's `media.db` into `restore-inspect-<pid>-<n>/`;
- journal writes go through `.restore-journal-<pid>-<n>.tmp`;
- import stages the new generation in `restore-staging/`.

Each operation removed its own temporary only when it returned. A process killed in between (OOM, logout, power loss) left a database copy that no later run reused, because the names are random, and startup recovery looked only at `restore-journal.json`. Separately, a restore that had staged successfully but then failed the live checkpoint, the live sidecar removal, or the `swapping` journal write returned through `?` and left the whole staging tree, because no journal existed yet for startup recovery to act on. On a disk that the extraction had just filled, that is exactly the likely failure.

The profile directory belongs to one identifier, and `InstanceLock` is held before setup touches it, so nothing else can be using these names when startup runs.

## Options considered

1. **Startup sweep of exact names created by this module (chosen).** Covers every crash leftover in the profile without changing how the operations work. Risk: deleting something the user put there. Bounded by matching only direct children, exact prefixes and suffixes, a decimal `<u32>-<u64>` token, and the expected file type, and by never following or removing symbolic links.
2. **Fixed names that the next call removes first.** Rejected: an export and an inspection can be left over together, and a leftover stays until the user happens to repeat the same operation, which may be never.
3. **Write the temporaries to the system temp directory.** Rejected: the snapshot and the extracted database can be gigabytes, `/tmp` is often a size-limited tmpfs, and a crash leaves them there just the same.
4. **Sweep by prefix only (`backup-snapshot-*`, `restore-*`).** Rejected: a looser pattern can remove a file the user named that way, and `restore-*` would also match the recovery generations that must survive.

## Decision

- `remove_abandoned_backup_temporaries` runs in setup directly after `recover_interrupted_restore` succeeds and before `thumbs/` is created or `media.db` opened. It removes regular files `backup-snapshot-<pid>-<n>.db`, `.db-journal`, `.db-wal`, `.db-shm` and `.restore-journal-<pid>-<n>.tmp`, directories `restore-inspect-<pid>-<n>`, and the directory `restore-staging` only when neither `restore-journal.json` nor a `*.restore-previous` artifact exists. Every other entry, every symbolic link, and every entry of an unexpected type is left alone. Failures are logged and never fail startup. Every creation site and the matcher share `temporary_token`, so a new name format cannot drift from the sweep unnoticed.
- The sweep is startup-only. It is not part of `recover_interrupted_restore`, which also runs at the end of every import, while an inspection may be running.
- In `import_db_bundle`, the live checkpoint, the live sidecar removal, and the `swapping` journal write share one cleanup boundary. A failure there removes `restore-staging`. If the journal was already renamed into place (only its directory sync failed), the normal journal recovery runs instead, so the journal and staging are removed together and the untouched live generation stays.
- `write_restore_journal` removes its temporary file when writing or publishing fails.

## Consequences

- A crash during export, inspection, or pre-swap staging no longer leaks a database copy in the profile beyond the next start. The startup order gains one best-effort step, documented in the system overview.
- A `restore-staging` tree next to an unexplained previous generation is still kept, so manual preservation instructions keep working.
- The partial `.<name>.<pid>-<n>.tmp` export archive next to a user-chosen target is not swept; the app does not know that folder after a crash.
- Adding a new temporary in the profile directory now means adding its exact pattern to the sweep and its test.
- No schema, bundle-format, or IPC change.
- Tests in `services/backup_service.rs`: `temporary_names_written_by_this_module_match_the_startup_sweep`, `startup_sweep_removes_only_abandoned_backup_temporaries`, `startup_sweep_keeps_staging_while_restore_artifacts_exist`, `failed_journal_write_removes_its_temporary_file`, `pre_swap_failure_removes_restore_staging_and_keeps_the_live_library`, `abandoned_staged_restore_rolls_back_a_published_journal`.
