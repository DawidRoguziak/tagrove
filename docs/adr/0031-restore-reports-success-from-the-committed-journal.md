# 0031: A bundle restore is a success from the `committed` journal rename on

- Status: Accepted
- Date: 2026-11-11
- Areas: data safety (bundle restore, restore journal, startup recovery), IPC (`import_db_bundle` result)
- Related: [data safety](../subsystems/data-safety-and-portability.md#restore-rollback-and-recovery), [IPC contract](../architecture/ipc-contract.md), [ADR 0016](0016-startup-sweep-of-abandoned-backup-temporaries.md), [ADR 0030](0030-restore-carries-the-live-file-mutation-journal.md)

## Context

Bundle restore (`import_db_bundle` in `src-tauri/src/services/backup_service.rs`) swaps generations under `restore-journal.json`. Recovery (`recover_interrupted_restore`) rolls back while the journal says `swapping` and keeps the new generation once it says `committed`.

The `committed` journal was written inside the block whose errors mean "roll back". Its write is a rename followed by a directory sync. When the rename landed and the sync returned an error, the error branch ran recovery, which read `committed`, kept the new generation and deleted the previous one, and the command still returned the sync error. With a sync that kept failing the text was "Automatic rollback also failed", although nothing had been rolled back. The cleanup after a successful commit had the same shape: any failure in it was returned as the command's error. The user saw a failed restore with the new library installed. Found in the triage of the audit run of 2026-11-08 (T1, lead 11).

A directory sync that fails says nothing certain about the rename: it may or may not be on disk. A second sync that passes does not prove it either, because the kernel reports a writeback error once.

## Options considered

1. **Success from the rename on; skip the cleanup when the journal sync failed (chosen).** The result matches what the running application and, in every case but a power loss with a lost rename, the next start will use. The previous generation stays as the rollback target while the commit is not known to be durable.
2. **Success from the rename on; always run the cleanup at once.** This is what the old error branch did in effect. Rejected: it deletes the previous generation while the `swapping` journal may still be the durable one. A power loss could then leave a `swapping` journal with one previous component gone and the other present, and recovery would combine a new database with old thumbnails.
3. **Retry the sync and continue when the retry passes.** Rejected: a passing retry is not evidence that the first entry reached the disk, and the gain is only that a second restore needs no restart.
4. **Keep returning an error, with a text that says the new library is installed.** Rejected: the frontend treats a rejection as a failed restore, and the summary counts would be lost. The state the user has to act on (none) does not fit an error.
5. **Add a warning field to `DbBundleImportSummary`.** Rejected for now: it is a change through every IPC layer and new UI copy in eleven locales for a state that needs no user action. One stderr line records it. It can be added later without changing this decision.
6. **Treat a failed journal sync as "not committed" and roll back in process.** Rejected: the rollback needs the same directory syncs that just failed, and the `committed` journal is already the one recovery reads.

## Decision

- The rename that publishes the `committed` journal is the commit point of a restore. `publish_restore_journal` returns an error only when the rename did not happen; that error, like every earlier one, rolls back and is returned.
- After the rename, `import_db_bundle` cannot fail. `finish_committed_restore` syncs the profile directory and then runs the usual committed cleanup. It returns nothing; a failure writes one line to stderr.
- When the directory sync fails, the cleanup is skipped: the journal, `media.db.restore-previous`, `thumbs.restore-previous` and the empty `restore-staging` stay for the next start. The sync is not retried.
- When a cleanup step fails, the rest of the cleanup stays for the next start.
- The command then clears the query sessions and returns the same `DbBundleImportSummary` as an undisturbed restore.
- The `swapping` journal write keeps its rename-plus-sync contract (`write_restore_journal`), and a failure of its sync still rolls back.

## Consequences

- `import_db_bundle` rejects only when the previous library is the live one, or will be after recovery. "Automatic rollback also failed" now appears only for a rollback.
- After a post-commit failure the profile keeps recovery artifacts while the application runs on the new generation. Another import in the same run is refused with the existing "recovery artifacts already exist; restart" error. The next start finishes the cleanup through the unchanged `recover_interrupted_restore`.
- Accepted risk: when the journal sync failed and the rename never reaches the disk, a power loss makes the next start roll back to the previous library, including anything written to the new one in between, although the restore had reported success. The alternative results were a wrong error (before) or a possible mix of generations (option 2).
- Not closed: the next start removes the previous generation of a `committed` journal without evidence that the rename is durable either. If the rename was lost in writeback although later syncs passed, the application was restarted without a reboot, and power fails during that cleanup, a `swapping` journal could meet a partly removed previous generation. This needs a failed sync, a lost rename and a power loss inside a short window, and is left as it was before this change.
- The pending cleanup has no user-interface surface, only stderr.
- IPC shape, bundle format, schema, journal format and startup order are unchanged.
- Tests in `services/backup_service.rs` use the `sync_fault` hook of ADR 0030 with a new lasting variant (`FailFrom`): `restore_with_a_published_committed_journal_reports_success`, `restore_cleanup_failure_after_the_commit_point_reports_success`, `restore_failure_before_the_commit_point_keeps_the_previous_generation`. They do not model a failing rename or real power loss.
