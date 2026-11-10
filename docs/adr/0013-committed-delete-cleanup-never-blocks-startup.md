# 0013: Committed-delete cleanup never blocks startup and keeps its journal row until the file is verifiably gone

- Status: Accepted
- Date: 2026-11-10
- Areas: data safety, file-mutation journal, startup recovery, scanning (root removal), clear library
- Related: [data safety](../subsystems/data-safety-and-portability.md#file-mutation-commit-and-recovery), [database](../subsystems/database.md)

## Context

A delete stages the source as a hidden `.mediatagger-*.pending` sibling, commits the database change, and then removes the staged file. When that last step does not finish (a crash after the commit, or a removal or directory-sync error reported as `cleanup_pending`), the committed `pending_file_operations` row is the only record of the leftover file. Startup recovery (`recover_pending_file_operations` in `src-tauri/src/services/asset_mutation_service.rs`) retries the cleanup before the application state exists.

The audit run of 2026-11-08 found two defects in that retry:

- The scan-root confinement check for an existing staging file was propagated as a startup error. Root removal and clear-library do not touch the journal, so once the covering root was gone every later launch failed until the hidden file or the journal row was removed by hand.
- `NotFound` was accepted as "cleaned up" even when it came from a missing parent directory (unmounted volume, renamed folder). The row was dropped, and the deleted content stayed on disk with nothing left to retry its removal. The post-commit cleanup of the running application had the same gap and reported `deleted`.

The owning page already said that cleanup-only delete failures keep the journal and do not prevent startup. The code did not hold that line in these two cases, and no test ran recovery against a non-empty journal.

## Options considered

1. **Skip and keep (chosen).** Treat every committed-delete cleanup failure, including a failed confinement check, as "keep the row, log, continue", and drop the row only when the staging file is verifiably gone. Startup always proceeds; nothing outside a scan root is ever deleted; the row keeps the recovery path. Cost: a row can stay for a long time and has no user-interface surface.
2. **Purge committed rows when a root is removed or the library is cleared.** Removes the startup failure at its trigger. Rejected: it discards the only record of a file that still holds deleted content, covers neither a tampered journal nor a symlink escape, and still needs option 1 for those.
3. **Refuse root removal and clear-library while the journal is non-empty.** Rejected: it blocks the two in-application ways to reset a library because of a leftover the user cannot see or resolve from the interface.
4. **Delete the staged file without the confinement check when no root covers it.** Rejected: the check is the guard against a tampered or stale journal deleting a path outside the indexed roots.
5. **Drop the row when the staging folder is unreachable** (previous behavior). Rejected: an unmounted volume is the common reason for an unreachable folder, and the file returns with the volume.
6. **Record a volume identity in the journal** (the staging folder's device id, or whether it was a mount point) and require it to match before a missing file counts as gone. Not done here: it needs a new `pending_file_operations` column and a backup-validation change, and device ids are not stable across boots for every filesystem or for re-plugged removable drives, so a naive comparison would strand rows. Left as follow-up work.
7. **Keep the row whenever the staging folder is empty.** Rejected: deleting the last file of a folder and then failing the directory sync or the journal delete would strand the row, because that folder stays empty.

## Decision

- A committed delete is cleanup-only work. No failure while finishing it stops startup: recovery writes one line to stderr, keeps the journal row and the file, and continues with the next row.
- A staging file that still exists is removed only after it canonicalizes below a current scan root that contains the journaled original path. A failed check keeps the row and leaves the file in place.
- A committed-delete journal row is dropped only when the staging file is verifiably gone: removed now or reported missing by `lstat`, inside a parent directory that can be reached and synced. The post-commit cleanup of the running application uses the same rule and returns `cleanup_pending` otherwise. The check covers the directory, not the volume behind it (see the first limitation below).
- Root removal and clear-library keep leaving `pending_file_operations` untouched.
- Uncommitted operations and committed renames are unchanged: an uncertain active-record/source mapping still stops startup.

## Consequences

- Removing a scan root or clearing the library can no longer make the application unstartable. A retained row is resolved by a later start after the root is registered again, or after the hidden staging file has been removed by hand.
- Known limitation: a staged file that sat directly in a mount-point folder still counts as gone when the volume is not mounted and that folder stays behind as an empty directory (a fixed `fstab` mount, a late network share). The row is dropped and the file returns with the volume. Files in a subfolder of the mount point, and mounts whose folder is removed on unmount, are covered. Closing this needs option 6.
- A retained row keeps bundle export rejected ("pending local file operations") and is reported only on stderr. A row whose staging folder never returns stays until that folder exists again at a later start. Surfacing retained rows in Settings, with a way to dismiss them, is possible follow-up work.
- A delete whose staging folder vanishes between the commit and the cleanup now reports `cleanup_pending` with the recovery path instead of `deleted`. The IPC contract is unchanged; the status already existed.
- New invariant: only a verifiably absent staging file ends a committed delete. It is enforced by `finish_committed_delete`, `remove_staged_delete`, and `staged_file_verifiably_absent`, and tested by the `recovery_*` unit tests in `asset_mutation_service.rs` and the `startup_*` restart tests in `src-tauri/tests/backend_integration.rs`.
- No schema, IPC, or backup-format change, and no migration.
