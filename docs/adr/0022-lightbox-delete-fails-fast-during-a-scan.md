# 0022: Lightbox delete fails fast while a scan holds the scan lock instead of waiting for it

- Status: Accepted
- Date: 2026-11-11
- Areas: data safety (deletion), locking, IPC (`delete_asset`), lightbox
- Related: [system overview, in-process locking](../architecture/system-overview.md#in-process-locking-policy), [lightbox](../subsystems/lightbox.md#persistent-edits-and-refresh-rules), [data safety](../subsystems/data-safety-and-portability.md), [IPC contract](../architecture/ipc-contract.md), [ADR 0014](0014-single-asset-file-mutations-guarded-by-row-cas.md)

## Context

`delete_asset` ran `delete_asset_service` through `with_scan_and_thumb_lock` (`src-tauri/src/app/locks.rs`), which blocks on `scan_lock` with no deadline. The startup scan, a Settings rescan and the other combined workflows hold `scan_lock` for their whole run (`src-tauri/src/commands/scan.rs`), which takes minutes on a large library. While the delete promise is pending the lightbox (`src/features/lightbox/LightboxModal.tsx`) disables every exit on purpose: Close, Escape, the backdrop, keyboard navigation and the confirmation's Cancel, so a destructive request cannot be orphaned by its own dialog. Together the full-window modal stayed locked until the scan ended, and the asset's per-asset write turn stayed held. The audit of 2026-11-08 recorded this as `S-a4965f13`.

## Options considered

1. **Fail fast (chosen).** The delete tries `scan_lock`; when it is held, the command rejects at once without staging or writing anything, and the confirmation shows a localized "scan or another library operation is running" message with Cancel, Close and navigation usable. The user retries after the scan. Simple, nothing is left pending, and no lock is held across `.await`.
2. **Keep Cancel and Close usable while the delete waits.** Rejected: the blocking worker cannot be cancelled once it waits for the mutex, so Cancel would close the dialog while the file is still deleted minutes later, after the user has moved on. A destructive action that happens long after its confirmation, with no visible owner, is worse than a clear refusal. It also keeps the per-asset write turn held for the length of the scan.
3. **Wait with a deadline (`try_lock` in a loop for a few seconds).** Rejected: it only shortens the freeze, and the outcome still depends on timing the user cannot see. Combined workflows other than scans are short, so a bounded wait buys little.
4. **Disable Delete in the frontend while a scan runs.** Not sufficient alone: the frontend does not own scan state for other windows or direct IPC callers, and the check would race with a scan that starts after it. The backend refusal is the authority; a frontend hint can be added later on top of it.

## Decision

- `try_with_scan_and_thumb_lock` in `src-tauri/src/app/locks.rs` takes the operation permit, then `try_lock` on `scan_lock` (a poisoned lock is recovered as everywhere else), then the exclusive `thumb_lock` with the normal blocking write. It returns `Ok(None)` without running the action when `scan_lock` is held. The lock order is unchanged.
- Only the thumbnail write lock waits: its holders are short read sections of thumbnail work and `clear_all_thumbnails`, which has no UI entry point.
- `delete_asset` uses it and rejects with the display text `i18n:common.libraryBusy` (`LIBRARY_BUSY_ERROR` in `src-tauri/src/commands/assets.rs`), in the existing backend-text format. The frontend only translates it for display (`translateBackendText` in `src/i18n/backendText.ts`, used by `LightboxDeleteConfirmDialog`); no code branches on it.
- Duplicate resolution, root removal, clear-library and bundle work keep the blocking helpers.

## Consequences

- A lightbox delete during a scan returns within milliseconds, leaves the source file, its row and the journal untouched, and the lightbox stays usable. The user has to retry after the scan.
- The same refusal applies while root removal or clear-library runs, which is why the message names "another library operation".
- `delete_asset` gains a new rejection case; the command name, arguments and `DeleteAssetSummary` are unchanged. Tested by `delete_fails_fast_while_a_scan_holds_the_scan_lock_and_changes_nothing` and `delete_recovers_a_scan_lock_poisoned_by_a_panicked_holder` in `src-tauri/src/commands/assets.rs`, and in the lightbox by "shows a busy-library delete rejection localized and keeps every exit usable" in `src/features/lightbox/__tests__/LightboxModal.test.tsx`.
- Follow-up, out of scope here: the duplicate-resolution apply in Settings still waits for a running scan, and every command's operation permit still waits for the bundle maintenance gate.
