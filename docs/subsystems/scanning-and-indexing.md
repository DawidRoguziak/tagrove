# Scanning and indexing

This page documents the scanning behavior implemented today. The Rust code and tests are authoritative; planning documents are not evidence of current behavior. For surrounding contracts, see [system architecture](../architecture/system-overview.md), [IPC](../architecture/ipc-contract.md), [database persistence](database.md), and [thumbnail generation](thumbnails.md).

## Root lifecycle

The registered commands expose four root operations and two ways to start a scan:

| Operation | Current behavior |
| --- | --- |
| `add_scan_root(path)` | Normalizes the path, requires an existing directory, and inserts it into `scan_roots`. The insert is idempotent for the exact stored string. It does not scan or bump the library revision. |
| `list_scan_roots()` | Reads roots in database path order, then sorts directories with available filesystem creation times newest-first. Paths without a creation time follow those with one; ties and missing-time entries sort lexicographically. This is filesystem creation order, not database insertion order. |
| `scan_folder(path)` | Runs on Tauri's blocking pool, takes the scan lock, normalizes and validates the directory, persists the root, and scans only that root. Persistence happens before scanning, so a later scan error can leave the root registered. |
| `rescan_all_roots()` | Runs on the blocking pool, takes the scan lock, reads every stored root, and scans them sequentially. A root that no longer exists is skipped and makes the result partial. No roots returns a complete zero-count result without opening the scan database, bumping the revision, or optimizing it. |
| `remove_scan_root(path)` | Takes the scan lock and exclusive thumbnail lock in that order, normalizes the path, and transactionally removes the root, its cascading mappings, assets with no remaining root mapping, stale thumbnail-failure rows, and orphan tags. It then bumps the revision and best-effort deletes thumbnail files for removed assets. |

Removing an unknown root is therefore a successful no-op at the root/asset level, but the command still bumps the library revision. `RemoveRootSummary.removed_assets` counts database assets deleted, while `removed_thumbnails` counts only thumbnail files whose `remove_file` call succeeded after the database transaction.

## Path identity and discovery

`normalize_root_path` trims surrounding whitespace and removes trailing `/` except for the filesystem root `/`. The command validates the resulting `PathBuf` with `exists()` and `is_dir()`.

The normalizer does not canonicalize `.` or `..`, resolve symlinks, expand relative paths, case-fold components, or obtain a canonical spelling. SQLite keys roots and assets by their stored text, so alternate spellings can become distinct root or asset identities.

Discovery recursively uses `WalkDir::new(root).follow_links(false)`. Entries that are not regular files are ignored, so symbolic-link entries are not indexed and linked directories are not traversed. An unreadable or otherwise failing walk entry increments `traversal_errors`, traversal continues where possible, and the discovery report becomes partial. A failure returned by the visitor itself is different: it aborts discovery and propagates as a command error.

Extensions are matched case-insensitively after lossy conversion to lowercase:

| Stored kind | Supported extensions |
| --- | --- |
| `image` | `jpg`, `jpeg`, `png`, `webp`, `bmp` |
| `gif` | `gif` |
| `video` | `mp4`, `mkv`, `webm`, `mov`, `avi` |

File contents and MIME signatures do not participate in kind detection. Unsupported files are invisible to the scan and do not contribute to progress totals.

## Fingerprints and extracted metadata

For every discovered supported file, the discovery thread reads filesystem metadata and forms a fingerprint from:

- `size_bytes`, cast from the metadata length to `i64`;
- `modified_at`, whole seconds since the Unix epoch; and
- `modified_at_ns`, the same timestamp in nanoseconds, capped at `i64::MAX`.

A pre-epoch modification time is treated as zero because `duration_since(UNIX_EPOCH)` falls back to its default duration. Incremental comparison uses the stored kind, size, and nanosecond timestamp. The seconds value remains the public/database `modified_at` used by ordering and thumbnail versioning; `fingerprint_mtime_ns` exists specifically for the finer-grained scan comparison.

Changed and new images and GIFs receive a best-effort width and height from `image::image_dimensions`. Decode/header failure leaves both values absent but does not fail indexing. Videos receive a best-effort duration in milliseconds from the thumbnail module: matching `ffprobe` candidates are tried first, then ffmpeg stderr parsing is used as a fallback. Each tool invocation has a 12-second timeout. A missing tool, timeout, non-zero/invalid probe result, or unparseable duration leaves `duration_ms` absent and still does not fail indexing. See [thumbnails](thumbnails.md) for the shared media-tool behavior.

## Incremental pipeline and limits

Each valid root is processed as one streaming pipeline:

1. A per-root generation is derived from the current Unix timestamp in nanoseconds, capped at `i64::MAX`; the root's zero-based position is added with saturation.
2. The discovery thread walks supported files, reads their fingerprints, and accumulates discovery batches of at most **512** files.
3. One SQL query per discovery batch fetches existing fingerprints by exact paths. An unchanged `(kind, size_bytes, modified_at_ns)` asset bypasses metadata probing and is queued for a root-generation touch. Everything else enters the worker task queue.
4. The task queue is a bounded synchronous channel with capacity **1,024**. The scan starts `min(available_parallelism, 8)` workers, with a minimum of one; the current streaming call supplies an unknown/unbounded file count, so the discovered count does not reduce this number. Workers share the task receiver and inspect files concurrently. The result channel is also bounded, at **1,544** entries (`1,024 + 512 + 8`), so workers apply backpressure instead of accumulating unbounded metadata.
5. Unchanged IDs are flushed when their accumulator reaches **512**, but a whole discovery batch is appended before that check; one touch transaction can therefore contain **512–1,023** IDs, with a final transaction of **1–511**. Successful changed/new results commit in transactions of at most **512** assets. Each changed upsert writes the asset metadata, exact nanosecond fingerprint, and `(asset_id, root_path, generation)` mapping.
6. During discovery, ready worker results are drained so database writes and indexing overlap the walk. After discovery, the sender is closed, remaining unchanged touches and worker results are consumed, the last partial write batch is committed, and all workers are joined.
7. Only a complete root enters generation pruning. After all supplied roots, a non-empty scan bumps the library revision once and runs `PRAGMA optimize` once.

`ScanSummary.indexed` includes both unchanged files successfully touched for the root and changed/new files successfully written. It is therefore a processed-success count, not a count of newly inserted or materially changed rows.

## Root generations, overlap, and cleanup

`asset_scan_roots` is the ownership/membership relation between one asset and every root that has seen it. Its primary key is `(asset_id, root_path)`, and `last_seen_generation` records the latest successful sighting for that root. Both foreign keys cascade: deleting an asset removes all mappings, and deleting a root removes that root's mappings.

This relation makes overlapping roots safe. If both `C:\media` and `C:\media\photos` discover the same exact asset path, the single `assets` row receives two mappings. Completing a later scan of one root deletes only that root's mappings from older generations. The asset row is deleted only when no mapping from any root remains. Root removal follows the same orphan rule.

A root is complete only when all of these are true:

- `WalkDir` reported no traversal errors;
- every supported discovered file produced a fingerprint; and
- every queued changed/new file produced an indexed item.

For a complete root, pruning transactionally deletes mappings for that root whose generation is stale, deletes globally orphaned assets, deletes thumbnail failures for missing assets, removes orphan tags, and conditionally bumps the revision when an asset row was removed. A revision failure rolls back that prune. `ScanSummary.removed` counts deleted asset rows, not stale mappings. For a partial root, pruning is skipped entirely, so older mappings and assets are preserved; successful touches and upserts from that same partial pass remain committed.

Generation pruning does not delete thumbnail files belonging to assets removed by a completed scan. In contrast, explicit root removal collects orphan thumbnail paths in its database transaction and tries to delete those files afterward.

## Progress and results

Scanning broadcasts the shared `process-progress` event described by the [IPC contract](../architecture/ipc-contract.md). Its payload is `{ phase, processed, total, message }`; delivery failures are ignored, and there is no request ID or sequence number.

| Phase | When it is emitted |
| --- | --- |
| `scan-empty` | Once when no roots are supplied, with zero counts. |
| `scan-skip` | Once for each missing/non-directory stored root; counts are the root position and number of roots. |
| `counting` | Every 100 discovered supported files while walking. Both `processed` and `total` are the current, still-growing discovered count. |
| `scanning` | While consuming changed-file results, every 100 processed files and at the end of the changed queue. `processed` includes unchanged touches plus completed changed tasks; `total` is the final supported-file discovery count. A root with no queued changes may emit no `scanning` event. |
| `cleanup` | Once after each valid root, reporting either completed cleanup and its removed count or partial completion and the warning count. |

`ScanSummary.completion` is `complete` unless at least one supplied root is invalid or at least one valid root is partial. `failed` adds invalid-root skips, traversal errors, fingerprint failures, and failed worker results. `removed` sums assets orphaned by complete-generation pruning. The command result, not progress delivery, is the completion signal.

## Concurrency, failure, and commit boundaries

`scan_folder` and `rescan_all_roots` move blocking traversal, probing, thread coordination, and SQLite work off Tauri's async executor with `spawn_blocking`. Both hold the process-wide scan mutex for the entire workflow, so they cannot overlap another scan or a combined destructive operation. Root removal also holds the exclusive thumbnail lock because it deletes thumbnail files. Adding and listing roots take no workflow lock and can overlap a running scan. The broader lock order is documented in [system architecture](../architecture/system-overview.md).

The scan is not one database transaction. Unchanged touches, each changed write batch, each complete-root prune, the final revision bump, and optimization have separate commit boundaries. Consequently:

- a warning-level traversal, fingerprint, or per-file indexing failure returns a successful partial summary; completed batches remain stored and stale cleanup for that root is skipped;
- a hard database, queue, worker-disconnect, thread-spawn, or worker-panic error fails the command; earlier transactions remain committed, later roots are not processed, and the final revision bump and optimization may not run;
- a final revision-bump failure can leave earlier touch/write transactions committed, but changed write batches and destructive prunes each carry their own transactional revision bump;
- an optimization failure happens after the revision bump and turns the command into an error even though scan data and the new revision are already committed; and
- explicit root removal commits its database deletion and revision together before best-effort thumbnail deletions.

Errors cross the IPC boundary as strings. `spawn_blocking` join failures are reported as `scan worker failed: ...` or `rescan worker failed: ...`; internal scan-worker disconnection and panic paths have their own string messages. There is no rollback spanning SQLite, source metadata reads, external video tools, and thumbnail files.

## Current guarantees

- Valid command-added roots are normalized consistently before storage; invalid directories are not added by `add_scan_root` or `scan_folder`.
- Supported discovery is recursive, case-insensitive by extension, and does not follow symbolic links.
- Unchanged detection uses kind, byte size, and nanosecond modification time, avoiding unnecessary image/video metadata work for an exact fingerprint match.
- Per-root generation mappings preserve assets still covered by another overlapping root.
- A root is pruned only after warning-free discovery, fingerprinting, and changed-file indexing.
- Changed asset batches, unchanged touches, complete-generation pruning, and root removal each use SQLite transactions at their documented boundaries.
- Scans are serialized by the scan lock; operations needing both workflow locks acquire scan before the exclusive thumbnail lock.
- Every non-empty scan that reaches normal finalization bumps the library revision once and then requests SQLite optimization.

## Known limitations

- Path normalization is string cleanup, not canonicalization. Relative paths, case variants, dot segments, symlinks, and aliases can produce duplicate logical roots or fail exact path/mapping comparisons.
- SQL prefix compatibility/backfill helpers construct `LIKE` patterns without escaping `%` or `_` in root names. The generation pipeline itself matches discovered paths exactly, but legacy mapping backfill can overmatch such roots.
- Symlinked files and directories are intentionally absent from discovery; no option exposes follow-links behavior.
- Extension-only kind detection can admit corrupt or mislabeled files. Image dimensions and video duration are optional, and their probe failures do not contribute to `failed` or partial completion.
- A subsecond-only file change can be detected and reindexed through `fingerprint_mtime_ns`, while `upsert_asset` preserves an existing thumbnail whenever whole-second `modified_at` is unchanged. Thumbnail naming also uses whole seconds, so that change can retain a stale thumbnail.
- Generation values come from wall-clock time rather than a persistent monotonic sequence. The scan mutex prevents simultaneous scans in one process, but uniqueness is not enforced by SQLite.
- Scan pruning removes database rows but does not delete their thumbnail files. Reindexing a file whose whole-second modification time changed also clears the database thumbnail path without deleting the old file. Root removal does delete recorded thumbnails, but only best-effort after the database commit; failed deletions are not returned as errors.
- Progress is best effort, shared with other workflows, uncorrelated, and uneven for small or unchanged roots. Human-readable messages are not a stable API.
- The scan is not one transaction. Each changed batch and destructive prune invalidates sessions atomically, but a hard failure can still leave an intentionally partial sequence of committed batches; there is no resume journal or cancellation mechanism.
- Current focused tests do not execute the full Tauri `scan_roots` pipeline with overlapping roots, forced partial discovery, generation pruning, batch-boundary failures, worker panic/disconnect, or revision/optimization fault injection.

## Safe change checklist

1. When adding a media type, update extension-to-kind detection, decide whether dimensions or duration apply, verify frontend kind handling and thumbnail behavior, and update the [IPC contract](../architecture/ipc-contract.md) if the serialized vocabulary changes.
2. Preserve one stable path-identity policy across normalization, asset upserts, exact batch lookups, root mappings, migration/backfill SQL, removal, and frontend path conversion. Test drive roots, trailing separators, slash direction, case/alias behavior, and root names containing SQL wildcard characters.
3. Treat fingerprint changes as cache-invalidation changes. Keep incremental comparison, stored fields, thumbnail versioning, and thumbnail-failure versioning aligned; include a same-second/nanosecond-only modification test.
4. Preserve bounded backpressure and transactional batch sizes deliberately. If changing worker, queue, or batch limits, test zero/small inputs, more files than each boundary, slow workers, worker failure, and result-channel shutdown.
5. Keep discovery completeness explicit. Every new warning/failure source must either make the root partial or be documented as optional metadata; never run destructive generation pruning when any directory or supported file may have been missed.
6. For root-ownership changes, test parent/child and duplicate-spelling overlaps, rescanning each root in both orders, removing one root, removing the last root, and ensuring only globally orphaned assets/tags/failures are deleted.
7. Keep mutation and revision semantics aligned with [database persistence](database.md). Add fault tests at touch, upsert-batch, prune, revision, and optimize boundaries if those boundaries change.
8. Keep blocking work under `spawn_blocking`, scan serialization under the shared lock helper, and the combined lock order as scan then exclusive thumbnail. Consider direct IPC concurrency with unlocked add/list operations.
9. Keep progress phases and result fields synchronized with Rust models, `src/types.ts`, `src/api.ts`, and the [IPC contract](../architecture/ipc-contract.md). Consumers must not depend on display messages.
10. Review thumbnail cleanup whenever scan pruning or root removal changes. Database orphan deletion and filesystem cleanup need separate assertions because they have different failure semantics.

### Relevant existing tests

- `src-tauri/src/utils/paths.rs` covers platform-specific whitespace/slash normalization, trailing separators, and filesystem-root preservation.
- `src-tauri/src/indexer.rs` covers case-insensitive supported extensions, recursive supported-file collection, and success/failure for existing versus missing files.
- `src-tauri/src/services/scan_service.rs` covers worker-count resolution, the empty summary, root validity, and the older prefix-based stale cleanup helper.
- `src-tauri/src/commands/scan.rs` covers rejecting an invalid root, persisting a valid normalized root, and explicit root removal deleting an orphan asset and its thumbnail.
- `src-tauri/src/db.rs` covers prefix boundary safety and thumbnail preservation on unchanged whole-second timestamps, among broader schema/query behavior.
- `src-tauri/src/thumbs.rs` covers ffprobe/ffmpeg duration parsing and related duration/seek edge cases; these are parser tests rather than live-tool scan integration.
- `src-tauri/tests/backend_integration.rs` and `src-tauri/tests/backend_e2e.rs` exercise scan-root persistence as part of library clear and data workflows, but do not run the complete discovery/worker/generation pipeline.

Run the focused Rust unit tests plus the repository's backend integration suite after scanning changes. Tests that require actual filesystem traversal, timestamp resolution, permissions, symlinks, or media tools should assert the intended complete-versus-partial outcome.
