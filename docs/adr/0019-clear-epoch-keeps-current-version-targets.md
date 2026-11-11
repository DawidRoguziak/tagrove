# 0019: A result dropped by the clear epoch keeps a target whose source version is still current

- Status: Accepted
- Date: 2026-11-11
- Areas: thumbnails (publication cleanup, Clear all)
- Related: [thumbnails](../subsystems/thumbnails.md#generation-epoch), [ADR 0006](0006-thumb-uri-scheme.md), [ADR 0018](0018-render-all-keeps-thumbnails-of-unreachable-sources.md)

## Context

`clear_all_thumbnails` bumps `thumbnail_generation`, clears every `thumb_path` and sweeps the thumbnail directory. It does not touch scheduler jobs. Jobs are keyed by target path, and the target is derived from the source version only, so a `thumb://` request made after the clear for an asset whose pre-clear render is still queued or running joins that job as a second waiter. Both waiters receive the same file.

`publish_thumbnail_results` (`src-tauri/src/services/thumb_service.rs`) treated every result of an old epoch as stale and removed its produced file with a raw `fs::remove_file`. The pre-clear waiter therefore deleted the file the post-clear waiter published or was about to publish: the tile got `404`, the row could keep a `thumb_path` to a missing file, and the asset was rendered a second time. In one publication batch the stale set was also one set of asset IDs across all epochs, so the post-clear result was reported stale too. The removal also took the path from the result unchecked, although a Ready write can carry an untrusted recorded `thumb_path`.

## Options considered

1. **Put the epoch into the scheduler job key.** A post-clear request would start its own render of the same target. It doubles the render for exactly the case the dedup exists for, and two jobs would write the same final path.
2. **Cancel or re-key pending jobs on clear.** WebKitGTK cannot cancel `thumb://` requests and every request must be answered, so the waiters still need a result. It adds scheduler state for a rare maintenance action.
3. **Never remove a file of an old-epoch result.** Simple, but a render for a version that was also re-indexed after the clear would leave an orphan JPEG until the next Clear all.
4. **Remove a file of an old-epoch result only when its source version is no longer current.** The clear already swept the directory; a file written after it for the current version is a valid cache entry that the post-clear waiter or a later request publishes, as the demand path already does for an unreferenced target on disk.

## Decision

Option 4. Results are still never committed for an old epoch and are reported stale per epoch group, so a post-clear waiter of the same asset in the same batch is published and served. For an old epoch the rows are read once (`get_assets_for_thumbnails_by_ids`); a produced file is removed only when the row is gone or its source version differs from the result's. When the rows cannot be read nothing is removed. A compare-and-set loss in the current epoch still removes the file. Every removal at publication goes through the owned-file guard (regular file, not a symlink, inside the canonical thumbnail root), with the root canonicalized once per publication.

## Consequences

- A clear during gallery rendering no longer costs a 404 and a second render per tile that joined a pre-clear job.
- A pre-clear render that finishes after the clear can leave a current-version file without a database reference. The next request for the asset publishes it without rendering; nothing else changes, since such files already arise from bulk cancellation.
- An old-epoch publication costs one extra row read; it only happens after a clear.
- No schema, IPC or library-revision change.
- Clear all also counts the files in one walk and deletes during a second walk against a root canonicalized once, instead of buffering every path under the exclusive lock; the removal guard and counts are unchanged.
- Tests: `thumb_service.rs` unit tests share one controlled render job between a pre-clear and a post-clear request (same batch, pre-clear first, pre-clear last, re-indexed version) and check that stale cleanup removes only owned files; `tests/thumbnail_clear.rs` checks the two-pass sweep with files appearing or vanishing between the passes.
