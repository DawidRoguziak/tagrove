# Thumbnails

Implementation entry points: [rendering and tool probes](../../src-tauri/src/thumbs.rs), [scheduler](../../src-tauri/src/services/thumb_scheduler.rs), [publication and cleanup](../../src-tauri/src/services/thumb_service.rs), [frontend queue](../../src/hooks/useThumbnailQueue.ts).

This page owns JPEG preview identity, rendering, tool discovery, scheduling, publication, retries, cancellation, and per-tile updates. Thumbnail files and their database references have separate commit boundaries.

Command names, payload shapes, channel serialization, and shared progress-event rules live in the [IPC contract](../architecture/ipc-contract.md). Database schema and transaction conventions live in [database persistence](database.md), scan-time metadata and fingerprints in [scanning and indexing](scanning-and-indexing.md), gallery range loading and presentation in [library query and gallery](library-query-and-gallery.md), and app-data, resource, and lock lifecycles in the [system overview](../architecture/system-overview.md).

## Source versions, cache and target identity

Every thumbnail job carries a `SourceVersion` — the exact source identity of `path`, `size_bytes`, and nanosecond `fingerprint_mtime_ns` read from SQLite when the task was created. The version travels through the scheduler task, is echoed back in the result, and acts as the compare-and-set guard for every database write. The database compare-and-set rejects a result whose source version no longer matches. File publication happens earlier and stale produced files are removed best-effort after that check.

The thumbnail directory is `<app-data>/thumbs` and is created at startup. `thumb_target` hashes these bytes with SHA-256:

1. the cache schema version (`THUMB_CACHE_VERSION = 2`) encoded little-endian;
2. a zero separator byte;
3. the source path's lossy UTF-8 representation;
4. a zero separator byte;
5. the source's signed 64-bit `size_bytes`; and
6. the source's signed 64-bit `fingerprint_mtime_ns`.

The target is the lowercase hexadecimal digest plus `.jpg`. Asset ID, media kind, render settings, and file contents are not part of the key. The path makes one source version converge on one target across single-item, page, stream, and bulk entry points. A target already present on disk is treated as ready and its path is reconciled into SQLite through the same version guard.

Cache version 2 includes path, size, and nanosecond mtime. Existing database thumbnail paths are still reused while the referenced file exists; a cache-version bump alone does not invalidate those references. If changing the key or renderer, explicitly decide how to clear old references and remove unreferenced files. Current cleanup is reference-driven and does not sweep every old cache file.

When an incremental scan re-indexes a changed file whose seconds-resolution `modified_at` did not move (same-second modification), `upsert_scanned_asset` releases the stored `thumb_path` whenever the precise fingerprint value differs, so a superseded reference cannot survive the scan.

## Image and GIF rendering

Images and GIFs use the Rust `image` decoder and produce an RGB JPEG. Before a full decode, dimensions are probed from the file header; sources whose pixel count exceeds `MAX_DECODE_PIXELS` (50 megapixels) fail the job with a controlled error instead of allocating unbounded memory. The maximum normal thumbnail box is 390 by 390 pixels with Lanczos3 filtering. A portrait with `height / width >= 1.6` is handled specially: a square as wide as the source is cropped from `x = 0`, with its top at `min(96, height - width)`, and resized exactly to 390 by 390. Other images use aspect-preserving `thumbnail(390, 390)` behavior. The gallery applies `object-cover` when presenting the result.

Rendering creates the parent directory, removes any old sibling temporary file, and writes `<target-stem>.tmp.jpg`. Publication renames that file to the final target. If the final target appeared before publication, the temporary file is removed and the existing target wins. Callers see a target only after the JPEG write completed; temporary files are never stored in `assets.thumb_path`.

## Video probing and rendering

Video duration is collected during indexing. The probe tries derived `ffprobe` candidates in order and accepts the first finite, non-negative duration, rounded to milliseconds. If none succeeds, it invokes the resolved `ffmpeg` and parses the duration from stderr. Every probe process has a 12-second timeout. Pipe readers drain stdout and stderr while the child runs and retain at most 64 KiB per stream. Timeout/error paths terminate and reap the child before returning.

The render seek is chosen from the stored duration:

| Duration | Seek |
| --- | --- |
| missing | 1 second |
| non-positive | 0 |
| below 1 second | 80% of duration |
| 1 to below 5 seconds | 1 second |
| 5 to below 10 seconds | 1 second before the end |
| 10 seconds or more | 10 seconds, clamped just before the end |

The clamp keeps a known positive seek at most `duration - 0.001` seconds. Rendering runs `ffmpeg` with one decoder thread, no stdin, audio, subtitles, or data, selects one frame, scales it to width 390 while preserving aspect ratio, uses JPEG quality value 7, and writes through the same temporary-publication path as images. Each attempt times out after 45 seconds. When the chosen nonzero seek errors or creates no target, the scheduler makes one fallback attempt at zero seconds. There is no further retry inside that job.

Both probing and rendering poll child completion every 25 ms. A timed-out child is killed and waited on.

## ffmpeg and ffprobe discovery

At startup, ffmpeg discovery searches the resource directory and its `binaries` child, then equivalent executable-adjacent and `resources` locations. In each directory it considers `ffmpeg`; it then checks `/usr/bin/ffmpeg` and finally falls back to `PATH`.

For a probe, `ffprobe_candidates` tries a sibling name derived from the ffmpeg filename, a plain sibling `ffprobe`, bare `ffprobe` through `PATH`, then `/usr/bin/ffprobe`, removing duplicates. Failed probes fall back to the resolved ffmpeg's stderr duration. A missing or unlaunchable tool leaves duration absent or rendering failed without itself preventing startup.

`get_video_tool_status` uses the same candidate builder. It runs `-version` with a three-second timeout per candidate and returns separate availability booleans. Settings reads these once when its shell-lived scan controller mounts. This is an executable check, not proof that a particular file decodes or native playback works.

## Shared scheduler

One process-wide `ThumbnailScheduler` is created at startup. Its worker count is `available_parallelism - 2`, clamped to 2 through 8, or 4 when parallelism cannot be read. A constructor still enforces at least one worker for tests and direct callers. Startup refuses to launch the application when any worker thread failed to spawn: demand calls would otherwise block forever waiting for results that can never arrive.

The scheduler has FIFO high- and low-priority queues. Workers search the high queue first, but may pass a blocked video and run another eligible job. Internal single-item demand, internal page demand, and visible streamed IDs are high priority; streamed prefetch and bulk work are low priority. A later high-priority waiter promotes a queued low-priority target. Work already running cannot be preempted.

Jobs are keyed by the target path string. Enqueuing an existing target job attaches a waiter rather than starting another render, up to 256 waiters per target; the next enqueue fails fast. Each waiter retains its own asset ID and receives the shared result. This de-duplicates work across entry points while bounding fan-out memory. The scheduler permits at most two video jobs at once; image/GIF jobs may use the rest of the worker pool. Completion removes the job, decrements active-video accounting, wakes blocked workers, and sends the result to all waiters. A dropped receiver is ignored.

The processor uses `catch_unwind` so a job panic becomes a failed result and the worker loop continues. `MAX_PENDING_JOBS = 2048` bounds the queue; further enqueues fail with a queue-full error. Demand waits also have the deadlines described below. These controls do not make decoding or process shutdown infallible.

## Demand APIs and streaming limits

All demand paths first trust an existing database thumbnail path only when that file still exists. Otherwise they derive the current target from the asset's source version, reuse it if present, or schedule rendering. Every result is published through a compare-and-set write guarded by the source version: `update_asset_thumbnail_path_if_version_matches` (single) and `update_asset_thumbnail_paths_batch_versioned` (batch) update `assets.thumb_path` only when the record's path, size, and fingerprint still match the snapshot. A lost race reports `VersionMismatch`, and the caller attempts to remove the produced file. Cleanup failures can leave an unreferenced target. Success clears the recorded failure for the asset version; a missing source or failed render clears the stored path and records a failure.

- `ensure_asset_thumbnail` is an internal, unregistered Rust helper with no frontend wrapper. It handles one asset at high priority and waits for its scheduler result with a 180-second budget (`DEMAND_JOB_TIMEOUT`); exceeding it returns a controlled timeout error instead of hanging. It returns `null` for an unknown asset, missing source, render failure, or a version lost to a concurrent re-index.
- `ensure_page_thumbnails` is also internal and unregistered. It de-duplicates the supplied IDs, clamps the batch to 256 unique IDs (`MAX_PAGE_BATCH`), and treats every found asset as high priority. Waiting uses a 180-second stall deadline. Its legacy `thumbnail-ready` emission is an implementation detail and is not part of the registered IPC contract.
- `ensure_thumbnails` is the production channel API. It retains positive, first-seen unique IDs, at most 64 from `visibleIds` and then at most 64 additional IDs from `prefetchIds`. Visible work is high priority and prefetch work is low priority, for a maximum of 128 assets.

The frontend uses its queue generation as `requestId` to reject late responses locally. The backend accepts requests independently of that hook-local number, including after a reload restarts it. The backend thumbnail-clear epoch separately prevents publication across a clear operation.

The stream sends `{ event: "ready" }` only after the matching source-version database update accepts the path. A rejected version becomes `{ event: "stale" }`, stays retryable, and does not enter the ready or failure counts. After processing, failed/stale IDs and one `done` count are sent. Channel-send errors are ignored. Hook-local request generations are not compared across frontend mounts. See the [IPC contract](../architecture/ipc-contract.md) for the serialized union.

## Bulk render, retry, and cancellation

Only one `render_all_thumbnails` or `render_failed_thumbnails` operation may coordinate work at a time. Both submit to the shared scheduler and keep at most 24 scheduler results pending. Actual execution remains bounded by the scheduler worker count, the two-video limit, and the scheduler-wide 2048-job queue cap.

Both bulk modes fetch keyset pages of at most 512 assets, including failure eligibility, up to the maximum asset ID observed at the start. They release read cursors/connections before waiting for render results. Render-all counts an existing current target as generated, records missing sources, and skips eligible failures. Retry-failed selects only failures whose `source_record_version` matches the asset, in asset-ID order. Success clears only that version's failure marker; an older worker cannot overwrite or clear a newer failure.

Each page drains its at-most-24 in-flight jobs and commits at most 512 collected path updates. The source compare-and-set and clear epoch reject obsolete updates; rejected IDs contribute to the separate stale count. The 180-second result-stall deadline remains in effect. Progress counts describe this evolving library; concurrent scans can alter eligibility after the initial count.

Cancellation is cooperative. `cancel_render_all_thumbnails` returns `false` when no bulk coordinator is running; otherwise it sets an atomic flag and returns `true` directly, without waiting for admission, workflow locks, or a blocking worker. The coordinator checks before each enqueue and while waiting for results at 50 ms intervals. On cancellation it stops enqueueing/waiting, drains results already ready, batch-commits the collected path updates, emits the mode's done phase, and returns a summary with `cancelled: true` and possibly `processed < total`.

Scheduler jobs already queued or running are not removed or killed by bulk cancellation. They may still publish files after the bulk command returns, but results not ready during the nonblocking drain are not included in that bulk summary or database batch; those late files have no database publication from the completed coordinator. Scheduler tasks retain their operation permit until their file work finishes, so database maintenance drains them too. A later demand call discovers such a target on disk and reconciles its path.

## Generation epoch

`clear_all_thumbnails` bumps the process-wide `thumbnail_generation` counter. Page/stream and bulk coordinators capture that epoch before processing and recheck it before their database batch. A changed epoch skips those writes and attempts to remove the collected files. Workflow locks normally serialize clear against active coordinators. Scheduler jobs left behind by cancellation can still create unreferenced files after a coordinator returns.

## Frontend queue and tile updates

`useThumbnailQueue` maintains separate sets for additive requests, visible gallery demand, gallery prefetch demand, and in-flight IDs plus a session-local failure set. The gallery replaces its pending demand on each range change, dropping offscreen work before dispatch; bulk selection and duplicate previews keep their additive requests. `queueThumbnailsByIds` ignores non-positive IDs, IDs with a known or pending path, IDs failed during the current queue generation, and IDs already queued or in flight. A 36 ms timer coalesces requests. Processing is sequential by frontend batch, with up to 64 total IDs per channel invocation. Visible gallery IDs are taken first, then additive requests, then gallery prefetch. The first two groups populate `visibleIds`; the last populates `prefetchIds` for the existing backend priority scheduler. Dispatched work cannot be preempted, so a new viewport can still wait for the current batch to finish.

As ready, stale, and failed messages arrive, the hook batches store publications behind a 24 ms flush. Production keeps no mirrored thumbnail-path or per-item rendering maps in shell state. The store publishes ready paths and cleared rendering flags together, notifying each changed tile once per flush. Ready, stale, and failed IDs leave the rendering state on flush, and command completion clears any remaining batch rendering flags. Stale IDs never enter the sticky failure set and may be requested again. The pending count is the sum of queued and in-flight IDs; it drives the gallery's generation status.

`resetThumbnailQueue` cancels pending UI/process timers, increments the local generation, clears additive, visible, prefetch, in-flight, failed, pending-update, and rendering state, and clears the `ThumbnailStore`. It cannot cancel an invoke or backend scheduler work. Event callbacks and invoke-error handling compare their captured generation, so late work from before reset cannot repopulate frontend state. Reset also enables a previously failed ID to be queued again.

`ThumbnailStore` owns thumbnail paths and rendering IDs, with subscriptions keyed by asset ID. Gallery tiles, bulk ordering previews, and duplicate previews consume per-item updates. Paths are retained for cached gallery pages and mounted preview subscribers; inactive paths and their versions are released on eviction or unsubscribe. Each changed asset gets a monotonically increasing in-memory version and only its listeners are notified. `GalleryTile` consumes that version with `useSyncExternalStore`, reads its effective path/rendering state directly, and can update a tile without rerendering the entire virtual grid. Subscriptions are stable across tile rerenders. Versions are released when an asset has no stored path, rendering state, or listeners, and a store-wide increasing counter prevents reused versions after resubscription. A tile shows the transparent placeholder on absence or image-load error and shows a spinner only while rendering with no ready path. Static GIF previews and other gallery presentation rules are covered in [library query and gallery](library-query-and-gallery.md).

## Locks, progress, cleanup, and database consistency

Generation, demand, bulk retry, and cancellation commands take the thumbnail read lock. `clear_all_thumbnails` takes the write lock, so it waits for active readers and excludes new ones while it clears. Root removal, asset delete/rename, database bundle operations, and library clearing use the combined scan lock then thumbnail write lock where they touch thumbnail ownership. The lock ordering and process isolation are defined in the [system overview](../architecture/system-overview.md).

All thumbnail commands run on Tauri's blocking thread pool (`spawn_blocking`): each command acquires its workflow lock inside the blocking task, so no lock is ever held across an await and no async runtime thread is occupied by scheduler waits or SQLite work. Workflow locks recover from poisoning (`into_inner`) instead of rejecting every later command after a panicked holder; the thumbnail processor's panic guard additionally keeps scheduler state unpoisoned in practice.

Every cleanup path treats `assets.thumb_path` as untrusted persisted data. Before deletion it canonicalizes the configured thumbnail root and candidate, rejects symlinks and non-files, and requires the canonical candidate to remain below the canonical thumbnail root. Missing, invalid, or outside-root paths count as unsuccessful best-effort cleanup and are never passed to `remove_file`.

Bulk runs emit start, mode-specific progress, warning, and done phases; progress is emitted every 25 processed assets and at completion. Page/stream demand emits `thumbs-page-start` and `thumbs-page-done`; due to the current double gate, intermediate `thumbs-page` events occur only at common multiples of 16 and 25 (every 400 items), plus completion. Cleanup reports every 25 paths and at completion. All use the shared best-effort `process-progress` broadcast; command/channel results remain authoritative.

Single demand updates the database through its version compare-and-set immediately after its result. Page/stream success updates commit before each ready event. Failed page updates commit at batch completion; bulk updates commit per candidate page after processing or cancellation drain, while failure rows are written or cleared as individual results are handled. Failure-bookkeeping errors are deliberately ignored; a final path-update error rejects the command. A compare-and-set that loses against a re-index (or a generation reset) skips the write and best-effort deletes the produced file. Existing files can therefore temporarily precede their database references. Clear all first clears database paths and failures, then walks the owned thumbnail directory without following symlinks and best-effort removes its regular files, including unreferenced old versions and temporary files. It removes empty nested directories after their files and retains the root. Every file still passes the canonical-root containment guard. Enumeration failures and deletion failures are best effort; the count includes only removed files, not directories or skipped symlinks. A failed file removal leaves an unreferenced file; the reported removal count includes only successful deletions.

Asset/root deletion and rename clear related rows and best-effort remove known thumbnail files. Successful completed scan cleanup removes stale asset and failure rows, but ordinary rescans do not sweep unreferenced old-version targets from the thumbnail directory. Thumbnail path writes and clears do not bump the library revision because they do not change query membership or order.

## Known limitations

- Thumbnail identity covers path, size, and nanosecond mtime under cache version 2. Content is not hashed, so changed bytes with unchanged identity reuse a target. A renderer/cache-version change needs explicit reference invalidation; existing recorded files are otherwise trusted and unreferenced old files are not swept.
- Bulk cancellation does not remove shared scheduler jobs. Queued work can consume CPU and publish files afterward. Only results collected by the coordinator receive its version-checked database update; later demand can reconcile an existing target.
- The scheduler has no shutdown protocol; workers live for the whole process lifetime.
- Video rendering is capped at two active jobs in code, but there is no focused test for that cap or for choosing a runnable image behind blocked videos. Process regressions now cover draining both output pipes, diagnostic caps, and timeout reaping; resource/PATH discovery and all fallback combinations remain unverified.
- Stream channel sends and progress broadcasts are best effort. Frontend generation checks discard abandoned responses, but rendering continues when the frontend drops the receiver.
- Frontend failures are sticky only until queue reset and are not surfaced per tile. There is no automatic backoff or retry in the production queue; reset or an explicit backend retry workflow is required.
- Temporary files left by process termination are not swept at startup. Failed best-effort deletion and rescan version changes can also leave orphan JPEGs, until Clear all thumbnails sweeps the owned directory. Ordinary scan and asset cleanup remains reference-driven.
- Filesystem publication and SQLite updates are not one atomic transaction. A crash between rename and compare-and-set leaves a valid unreferenced target; a cleanup deletion failure can leave an unreferenced file; a ready channel message follows its accepted database update.
- Failure-record and failure-clear database errors are swallowed, so a successful render result does not strictly guarantee that its retry marker was cleared, and a failed render does not strictly guarantee that its retry marker was persisted.
- Failure rows use `source_record_version`; migration discards old markers whose source version is unknown.
- The page-progress double gate makes intermediate progress extremely sparse. This is current behavior, not an intended sampling recommendation.

## Safe change checklist

1. Preserve one target identity across every entry point. The key already carries a cache version (`THUMB_CACHE_VERSION`); if identity inputs or rendering rules change, bump it and plan regeneration/cleanup of orphaned targets; test same-path versions, same-second modifications, and old database references.
2. Keep temporary writes and final publication in the same directory. Test decode/render failure, stale temporary files, an already-present final target, and concurrent waiters without exposing a partial JPEG.
3. For image changes, test the 390-pixel bound, aspect preservation, exact `1.6` crop threshold, 96-pixel top bias and clamp, GIF decoding, RGB/JPEG conversion, and small/zero-dimension edge behavior.
4. For video changes, test every seek boundary, unknown duration, zero-second fallback, scale/output arguments, nonzero exit, missing executables, probe parsing, 12/45-second timeouts, and child termination.
5. Keep ffmpeg and ffprobe discovery aligned. Exercise resource, executable-adjacent, `/usr/bin`, and system `PATH` layouts.
6. Preserve scheduler target de-duplication, waiter fan-out, low-to-high promotion, FIFO priority, image progress around blocked videos, worker bounds, the two-video cap, panic-safe terminal results, and the `MAX_PENDING_JOBS` queue bound. Startup must keep refusing to run when worker threads fail to spawn.
7. Keep the Rust stream event enum, TypeScript union, API wrapper, command registration, positive-ID filtering, exact 64-visible/64-prefetch caps, priority split, and one terminal `done` synchronized. Follow the [IPC contract](../architecture/ipc-contract.md) for any transport change.
8. Preserve frontend 64-item total batching, visible/additive/prefetch priority, replaceable gallery demand, 36 ms queue coalescing, 24 ms UI flushing, queued/in-flight de-duplication, failure suppression, generation checks, reset semantics, and atomic per-asset store notification. Test rejected invokes, messages after reset, unmount, and overlapping queue additions.
9. Treat cancellation as a partial commit. Test cancellation before enqueue, while below and at the 24-result window, during final drain, with image/video jobs still running, and verify summary counts, failure rows, published files, and subsequent reconciliation.
10. Keep lock order consistent for generation, clear, rename/delete, scan-root removal, library clear, and bundle replacement. Test that exclusive cleanup cannot race a demand reader and that best-effort filesystem failure leaves truthful counts and recoverable database state.
11. When changing failure policy, test count reset on asset-version change, stale-row cleanup, render-all skip, retry-failed ordering, success clearing, missing sources, and repeated failures. Use the source record version for failure writes and the precise fingerprint for target identity.
12. Run Rust formatting and focused `thumbs`, scheduler, service, database, and command tests; run the frontend queue and gallery tests; then exercise a packaged desktop build with real image, GIF, short/long video, system tool discovery, cancellation, and cleanup.

### Relevant existing tests

- `src-tauri/src/thumbs.rs` covers tall-image crop geometry, duration parsing, video seek boundaries, version-sensitive target hashing, and decode-budget boundaries.
- `src-tauri/src/services/thumb_scheduler.rs` covers in-flight target de-duplication, waiter fan-out, high-priority ordering, parallel execution, panic-safe terminal results, version echo in results, and the queue-full bound.
- `src-tauri/src/services/thumb_service.rs` covers stall timeouts, cancellation flag behavior, reuse of an existing thumbnail, file-removal counts, the scan-update-versus-completion race (stale render dropped and its file removed), clear-epoch publication drops, and stale stream results followed by a lower request ID after reload.
- `src-tauri/src/commands/thumbs.rs` covers the cancellation service's accepted/not-running states while maintenance and both workflow locks are held.
- `src-tauri/src/db.rs` covers thumbnail compare-and-set outcomes (applied, no-op repeat, re-index mismatch), stale-ID reporting for batch writes, same-second rescan reference release, GIF inclusion in bulk candidates, versioned failure filtering/counting, and failure cleanup on asset deletion.
- `src/hooks/__tests__/useThumbnailQueue.test.ts` covers queue de-duplication, failure suppression, streamed ready merging, stale-generation rejection, and 64-item frontend batches.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx` covers rendering indicators and gallery-level thumbnail status among the virtual-grid behaviors.

The real ffmpeg test covers probe/render output and subprocess draining. Isolated desktop workflows exercise thumbnail generation through the WebView. Cleanup and cancellation interleavings remain unit-level checks; see the [verification record](../development/backend-refactor-verification.md).
