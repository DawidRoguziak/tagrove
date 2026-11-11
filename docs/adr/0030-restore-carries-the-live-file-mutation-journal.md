# 0030: Bundle restore carries the live file-mutation journal into the restored generation

- Status: Accepted
- Date: 2026-11-11
- Areas: data safety (bundle restore, file-mutation journal, startup recovery), database (backup validation)
- Related: [data safety](../subsystems/data-safety-and-portability.md#bundle-import-and-replacement-sequence), [database](../subsystems/database.md), [ADR 0013](0013-committed-delete-cleanup-never-blocks-startup.md), [ADR 0015](0015-restore-tolerates-unresolved-thumbnails-and-nullable-file-name.md), [ADR 0016](0016-startup-sweep-of-abandoned-backup-temporaries.md)

## Context

A `pending_file_operations` row is the only record of a hidden `.mediatagger-*.pending` file next to the user's media. Two kinds of row outlive the operation that wrote them: a committed delete whose staged file could not be removed (`cleanup_pending`, kept by [ADR 0013](0013-committed-delete-cleanup-never-blocks-startup.md) until the file is verifiably gone), and a row of a batch whose rollback failed (`rollback_failed`: the user's own file may still sit under its staging name). Startup recovery (`recover_pending_file_operations` in `src-tauri/src/services/asset_mutation_service.rs`) is the only code that resolves them.

Bundle restore (`import_db_bundle` in `src-tauri/src/services/backup_service.rs`) replaces `media.db` as a whole. It validated that the bundle's journal was empty and never looked at the live one. After a successful restore the previous generation was deleted with its rows: a committed delete's staged file stayed in the media folder for good with no record, and for an uncommitted row the user's original file stayed hidden and the recovery that would have returned it never ran. Bundle export already refuses a database with a non-empty journal; restore did not look. Found in the triage of the audit run of 2026-11-08 (T1, lead 5).

The rows describe files on this machine, not library content: they are not part of what a bundle restores. Retained rows have no user-interface surface (a known limitation of ADR 0013), and no command resolves them while the application runs.

Under the maintenance gate an uncommitted row can only be the leftover of a batch whose rollback failed in this process: a batch in flight holds an operation permit that maintenance waits for, and an uncommitted row left by a crash is either resolved or stops startup before any command can run.

## Options considered

1. **Carry every live row into the restored generation (chosen).** Restore succeeds, and the next start handles the rows as it would have without the restore. Cost: the restored database can hold journal rows for assets and scan roots it does not know, which recovery already tolerates (ADR 0013).
2. **Refuse the restore while the live journal is not empty.** Rejected: retained rows have no user interface, so this adds a second blocked workflow (after export) that the user can neither see the reason for nor clear from the application. A committed delete whose folder never comes back would block restore forever.
3. **Refuse only for uncommitted rows and carry committed deletes.** Rejected: the only way out would be a restart, and a restart resolves the row equally well after the restore, because recovery of an uncommitted row reads only the journaled paths and no library row. When the files cannot be reconciled, the restart stops at startup either way; refusing the restore first changes nothing about that and only adds an error the user cannot act on inside the application.
4. **Carry only committed deletes and drop the rest.** Rejected: dropping an uncommitted row is the data-loss half of the finding (the user's file stays hidden). Dropping a committed rename would be harmless on disk, but a rule per row kind is harder to check than "restore does not change the journal".
5. **Resolve the rows during the restore** (run recovery on the live database before the swap). Rejected: these rows exist because the same filesystem step just failed or its folder is unreachable, the confinement check of a committed delete depends on the scan roots of a library that is about to be replaced, and it would move user files inside an operation that may still roll back.
6. **Copy the rows after the swap** (from memory or from `media.db.restore-previous` into the installed database). Works, because the `swapping` journal covers the window, but the installed generation would be incomplete for a moment and the last validation of the staged file would not see what is installed. Rejected in favour of completing the staged file first.

## Decision

- Restore copies every row of the live `pending_file_operations` table into the staged database, unchanged and with its `created_at`, in one transaction on a `synchronous=FULL` connection (`carry_live_file_operations`, `db::copy_pending_file_operations`). No row is filtered by action or by `committed`, and no path is rewritten: root mappings apply to the bundle's rows only.
- The copy runs after path rewriting and before the staged database's final checkpoint and validation, so before the `swapping` restore journal and before any live path moves. A failure removes the staging tree and leaves the live generation, with its journal, untouched.
- The bundle itself must still have an empty journal: the first validation of the extracted database rejects any row, as does inspection. The final staged validation and the post-install validation run every other backup check and require the journal to hold exactly the carried rows, in journal order (`db::validate_restored_generation`). `validate_backup_database` is unchanged for bundles.
- A non-empty live journal never makes restore fail on its own. Bundle export keeps rejecting a database with pending rows, so a restored library with carried rows is not exportable until a later start has resolved them, exactly as before the restore.
- Startup order and recovery rules are unchanged. `init_schema` and the schema are unchanged.

## Consequences

- Crash safety follows the existing restore journal. Before the `committed` phase every failure or crash rolls back to the previous generation, which still holds its own rows; the staged copy is deleted with the staging tree (by the rollback, or by the startup sweep of ADR 0016 when no restore journal exists yet). From `committed` on, the live generation is the staged file, which already held the rows durably before the first rename. No point exists at which a live generation lacks the rows.
- After a restore, the next start treats the journaled files as it would have without the restore, because recovery reads the journal and the filesystem, plus the scan roots for a committed delete:
  - a committed delete is finished when a scan root of the restored library covers the staged file; otherwise the row is kept, one line goes to stderr, and startup continues (ADR 0013). Registering the root again, or removing the hidden file by hand, lets a later start drop the row;
  - an uncommitted row returns the user's file to its original name, or is dropped when the file is already there;
  - a committed rename is dropped when its final file exists.
- No new startup stop is introduced, and none is removed: an uncommitted row or a committed rename whose files cannot be reconciled stops the next start of the restored library, as it would have stopped the previous one (a documented limitation). Before this change a restore silently discarded such a row together with the recovery it stood for.
- A file hidden by a failed rollback stays hidden until the next start, also when the restored library indexes its original path; a scan before that start sees the path as missing. The same holds without a restore.
- A hand-made bundle whose `pending_file_operations` table rejects the copied rows (an added constraint) fails the restore before the swap, and only when the live journal is not empty. Bundles written by this application are not affected.
- Bundle format, manifest, IPC and schema are unchanged. Bundles still never contain a journal.
- New invariant: a restore neither drops nor alters live `pending_file_operations` rows. It is enforced by `carry_live_file_operations` and `db::validate_restored_generation`, and tested in `services/backup_service.rs` (`restore_carries_a_committed_delete_and_a_later_start_finishes_its_cleanup`, `restore_carries_uncommitted_and_rename_rows_for_startup_recovery`, `restore_interrupted_at_any_step_keeps_the_journal_with_the_live_generation`, `restore_fails_before_the_swap_when_the_journal_cannot_be_carried`), in `db.rs` (`backup_validation_rejects_a_journal_that_restore_validation_requires`) and in `src-tauri/tests/backend_integration.rs` (`restore_keeps_a_committed_delete_until_a_later_start_can_finish_it`).
- The interruption test uses a test-only fault at this module's directory syncs (`sync_fault` in `backup_service.rs`): an error or an unwind at the n-th sync, for every n of the sequence. It does not model a failing rename, a rollback that itself keeps failing, or real power loss.
- Follow-up, unchanged from ADR 0013: retained rows still have no surface in Settings.
