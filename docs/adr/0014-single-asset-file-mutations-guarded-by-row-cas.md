# 0014: Single-asset file mutations are guarded by the row compare-and-set, not by the library revision

- Status: Accepted
- Date: 2026-11-10
- Areas: data safety, file-mutation journal, database (library revision), concurrency
- Related: [data safety](../subsystems/data-safety-and-portability.md#duplicate-identity), [database](../subsystems/database.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md), [ADR 0013](0013-committed-delete-cleanup-never-blocks-startup.md)

## Context

Lightbox delete (`delete_asset`) and the backend rename service (`rename_asset`) build a one-item batch and run it through the same journaled pipeline as duplicate resolution (`apply_file_mutation_batch` in `src-tauri/src/services/asset_mutation_service.rs`). That pipeline compared a library revision before the first file move and rejected the batch with "duplicate snapshot is stale; rescan before applying changes" when it had changed.

For duplicate resolution the revision is the resolver scan's snapshot, and the comparison is the documented guard: the batch was planned against that snapshot and must not leave a new duplicate name. The single-asset paths read the revision themselves a few statements earlier, in autocommit, and compared it again in a second autocommit read. They hold the scan and thumbnail locks, which tag, favorite and media-group writes do not take, so any such write on another asset that committed between the two reads failed the delete with a misleading "rescan" error. Nothing was stale: the asset's own row is checked by the path/record-version compare-and-set in the commit transaction (`db::apply_file_mutations`), which is what protects it. The audit of 2026-11-08 recorded this as `S-b7492069`.

## Options considered

1. **Compare the revision only for duplicate resolution (chosen).** The single-asset paths pass no snapshot revision and rely on the per-row compare-and-set. Removes the false failure and one query; keeps every check that protects the row.
2. **Read the revision once inside a transaction and pass it through.** Still fails whenever an unrelated write bumps the revision between the read and the commit, because the comparison is against the global revision. It would only narrow the window.
3. **Exclude user metadata writes during file mutations** (make tag/favorite/group saves take the scan lock). Rejected: it reverses ADR 0002's user-write priority and blocks saves behind every delete for a guard the row compare-and-set already provides.
4. **Keep the comparison for single-asset paths and retry on mismatch.** Rejected: the retry rereads the value the check is meant to protect, so the check proves nothing.

## Decision

- `apply_file_mutation_batch` takes the snapshot revision as `Option<i64>`. Duplicate resolution passes `Some(scan_revision)`; the batch is rejected as stale when the current library revision differs, and the final duplicate-name check runs. `delete_asset` and `rename_asset` pass `None` and read no revision before the commit.
- The guard for a single-asset mutation is the ID/path/record-version compare-and-set in the commit transaction, plus the validation of the current row, its source file and its assigned scan root before staging.

## Consequences

- A tag, favorite or media-group save on another asset can no longer fail a lightbox delete. A change to the asset itself (path or record version) still fails it before or at the commit, and a failure at the commit is rolled back as before.
- Duplicate resolution keeps rejecting a snapshot that is stale for any reason, including an unrelated metadata write. That is deliberate: its planning depends on the whole library's file names.
- Enforced in `validate_and_prepare`; tested by `single_asset_mutations_ignore_metadata_writes_committed_between_their_statements` and `duplicate_batch_still_rejects_a_stale_snapshot_before_touching_files` in `asset_mutation_service.rs`.
- No schema, IPC or backup-format change. `DuplicateResolutionBatchInput.scan_revision` keeps its meaning.
