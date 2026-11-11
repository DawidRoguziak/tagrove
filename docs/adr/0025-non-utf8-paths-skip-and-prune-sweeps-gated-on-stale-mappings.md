# 0025: Discovery skips non-UTF-8 paths without making the root partial, and prune runs the orphan sweeps only after deleting a stale mapping

- Status: Accepted
- Date: 2026-11-11
- Areas: scanning, database (scan membership, pruning), data safety (stale-row deletion)
- Related: [scanning and indexing](../subsystems/scanning-and-indexing.md#root-generations-overlap-and-cleanup), [database](../subsystems/database.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md)

## Context

Two scan-cleanup problems came out of the 2026-11-08 audit.

A supported file whose path is not valid UTF-8 (`S-ac902718`, reproduced as `Q-776b6211`) passed discovery, because kind detection reads the extension lossily (`src-tauri/src/indexer.rs`, `detect_kind`). The worker then rejected it in `build_asset` because asset paths are stored as UTF-8 text, and the rejection counted as a failed result. A failed result makes the root partial (`root_scan_complete` in `src-tauri/src/services/scan_service.rs`), so generation pruning never ran for that root again. Rows of files deleted outside the app stayed in the gallery for good, and every scan reported a warning. Nothing was logged, so the user could not tell which file caused it. `database.md` already said that discovery skips such paths. The code did not.

`prune_completed_scan_root_generation` (`src-tauri/src/db/scan_membership.rs`) ran three sweeps over the whole library after its root-scoped mapping delete, inside one `BEGIN IMMEDIATE` transaction, even when that delete removed nothing (`S-1f24aaf1`). Those sweeps were the orphan-asset anti-join, the missing-asset failure rows and the orphan tags. On a no-op rescan of R roots that is R full passes under the write lock, which favorite, tag and group writes then wait behind.

The scanning invariant says that a new warning source must either make the root partial or be documented. Destructive pruning must never run when a supported file may have been missed.

## Options considered

1. **Skip non-UTF-8 paths in discovery, with a capped stderr warning (chosen).** No row can ever exist for such a path, so pruning a root that contains one cannot delete the row of a file that is still present. The skip is counted (`DiscoveryReport.non_utf8_skipped`) but is neither a failure nor a reason to be partial.
2. **Keep treating them as failures.** Rejected. This protects nothing, because the file can never be stored, and it disables cleanup for the whole root until the user finds and renames the file.
3. **Store a lossy identity.** Rejected. A lossy path cannot be opened. Two different names can also collapse into one row.
4. **Gate the orphan sweeps on the stale-mapping delete removing at least one row (chosen).** This is cheap and exact for anything this prune could orphan. When the gate opens, the sweeps stay library-wide, so they still clean up unrelated orphans as before.
5. **Scope the asset sweep to the asset IDs whose mappings were just deleted.** Not chosen now. Every prune that deletes something would stop cleaning up older orphans. The tag and failure sweeps would need their own scoping. Option 4 already removes the no-op cost, which is the reported case.

## Decision

- `visit_supported_files` checks `Path::to_str()` after kind detection. A supported file whose path is not UTF-8 is skipped before fingerprinting and counted in `non_utf8_skipped`. It does not reach the visitor, the progress totals, `failed` or the completeness check.
- Per-file scan warnings go to stderr as `[scan] warning: ...` through `indexer::ScanWarningLog`, capped at `MAX_LOGGED_SCAN_WARNINGS` (20) lines per root plus one summary line. This covers traversal errors, skipped non-UTF-8 paths, fingerprint failures and failed worker results (`IndexResult.warning`). Logging never changes completeness.
- `prune_completed_scan_root_generation` commits and returns `0` right after the root-scoped mapping delete when that delete removed no rows. Otherwise it runs the three library-wide sweeps and the conditional revision bump as before.

## Consequences

- A root holding a non-UTF-8 name now completes and prunes normally. The file is never indexed, and the only signal is the capped stderr line. The scan summary, the IPC contract and the Settings status are unchanged.
- No skipped path can hide a stored row: roots are UTF-8 text, every path below a non-UTF-8 folder is itself non-UTF-8, and `build_asset` has always rejected non-UTF-8 paths, so no lossy (U+FFFD) row was ever written. The lossy lookup in `dispatch_discovery_batch` now only ever sees UTF-8 paths.
- Orphans that a prune did not create are cleaned up only by the next prune that deletes a mapping, or by root removal. Examples are an unmapped legacy row, or a synthetic database without mappings. Root removal always sweeps. Production asset writes create their mapping in the same transaction, and restore rejects unmapped assets.
- A no-op rescan costs one indexed delete per root instead of three library-wide passes under the write lock.
- Tested by `discovery_skips_non_utf8_media_paths_without_making_the_root_partial`, `indexing_a_non_utf8_path_fails_with_a_warning` and `scan_warning_log_caps_lines_per_root_and_resets_after_the_root` in `src-tauri/src/indexer.rs`. Also by `completed_scan_prune_sweeps_orphans_only_when_the_root_lost_a_mapping` in `src-tauri/src/db.rs` and by `scan_prune_with_non_utf8_names` in `src-tauri/tests/backend_integration.rs`.
