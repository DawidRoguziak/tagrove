# Thumbnails

This subsystem creates versioned JPEG previews, schedules demand and bulk work, persists paths and failures, and delivers per-asset updates to the virtual gallery. The implementation in `src-tauri/src/thumbs.rs`, `src-tauri/src/services/thumb_scheduler.rs`, `src-tauri/src/services/thumb_service.rs`, the thumbnail database functions, `src/hooks/useThumbnailQueue.ts`, and their executable tests is the source of truth.

Command names, payload shapes, channel serialization, and shared progress-event rules live in the [IPC contract](../architecture/ipc-contract.md). Database schema and transaction conventions live in [database persistence](database.md), scan-time metadata and fingerprints in [scanning and indexing](scanning-and-indexing.md), gallery range loading and presentation in [library query and gallery](library-query-and-gallery.md), and app-data, resource, and lock lifecycles in the [system overview](../architecture/system-overview.md).

## Current guarantees

### Source versions, cache and target identity

Every thumbnail job carries a `SourceVersion` — the exact source identity of `path`, `size_bytes`, and nanosecond `fingerprint_mtime_ns` read from SQLite when the task was created. The version travels through the scheduler task, is echoed back in the result, and acts as the compare-and-set guard for every database write. A render that finishes after its asset was re-indexed can never be published.

The thumbnail directory is `<app-data>/thumbs` and is created at startup. `thumb_target` hashes these bytes with SHA-256:

1. the cache schema version (`THUMB_CACHE_VERSION = 2`) encoded little-endian;
2. a zero separator byte;
3. the source path's lossy UTF-8 representation;
4. a zero separator byte;
5. the source's signed 64-bit `size_bytes`; and
6. the source's signed 64-bit `fingerprint_mtime_ns`.

The target is the lowercase hexadecimal digest plus `.jpg`. Asset ID, media kind, render settings, and file contents are not part of the key. The path makes one source version converge on one target across single-item, page, stream, and bulk entry points. A target already present on disk is treated as ready and its path is reconciled into SQLite through the same version guard.

Version 2 replaced the original key (seconds-resolution `modified_at` only). Targets produced by earlier builds are not referenced anymore and are regenerated on demand under their new names; old files become orphans until an explicit cleanup clears them. Bumping `THUMB_CACHE_VERSION` again requires the same migration consideration.

When an incremental scan re-indexes a changed file whose seconds-resolution `modified_at` did not move (same-second modification), `upsert_scanned_asset` releases the stored `thumb_path` whenever the precise fingerprint value differs, so a superseded reference cannot survive the scan.

### Image and GIF rendering

Images and GIFs use the Rust `image` decoder and produce an RGB JPEG. Before a full decode, dimensions are probed from the file header; sources whose pixel count exceeds `MAX_DECODE_PIXELS` (50 megapixels) fail the job with a controlled error instead of allocating unbounded memory. The maximum normal thumbnail box is 390 by 390 pixels with Lanczos3 filtering. A portrait with `height / width >= 1.6` is handled specially: a square as wide as the source is cropped from `x = 0`, with its top at `min(96, height - width)`, and resized exactly to 390 by 390. Other images use aspect-preserving `thumbnail(390, 390)` behavior. The gallery applies `object-cover` when presenting the result.

Rendering creates the parent directory, removes any old sibling temporary file, and writes `<target-stem>.tmp.jpg`. Publication renames that file to the final target. If the final target appeared before publication, the temporary file is removed and the existing target wins. Callers see a target only after the JPEG write completed; temporary files are never stored in `assets.thumb_path`.

### Video probing and rendering

Video duration is collected during indexing. The probe tries derived `ffprobe` candidates in order and accepts the first finite, non-negative duration, rounded to milliseconds. If none succeeds, it invokes the resolved `ffmpeg` and parses the duration from stderr. Every probe process has a 12-second timeout.

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

### ffmpeg and ffprobe discovery

At startup, ffmpeg discovery searches the resource directory and its `binaries` child, then equivalent executable-adjacent and `resources` locations. In each directory it considers `ffmpeg`; it then checks `/usr/bin/ffmpeg` and finally falls back to `PATH`.

For a probe, the code derives a matching sibling `ffprobe`, then tries `/usr/bin/ffprobe` and bare `ffprobe`, with duplicate paths removed. If those fail, the ffmpeg stderr fallback uses the already-resolved ffmpeg path. A missing or unlaunchable video tool therefore becomes an absent duration and, later, a thumbnail failure rather than preventing application startup.

### Shared scheduler

One process-wide `ThumbnailScheduler` is created at startup. Its worker count is `available_parallelism - 2`, clamped to 2 through 8, or 4 when parallelism cannot be read. A constructor still enforces at least one worker for tests and direct callers. Startup refuses to launch the application when any worker thread failed to spawn: demand calls would otherwise block forever waiting for results that can never arrive.

The scheduler has FIFO high- and low-priority queues. Workers search the high queue first, but may pass a blocked video and run another eligible job. Internal single-item demand, internal page demand, and visible streamed IDs are high priority; streamed prefetch and bulk work are low priority. A later high-priority waiter promotes a queued low-priority target. Work already running cannot be preempted.

Jobs are keyed by the target path string. Enqueuing an existing target job attaches a waiter rather than starting another render, up to 256 waiters per target; the next enqueue fails fast. Each waiter retains its own asset ID and receives the shared result. This de-duplicates work across entry points while bounding fan-out memory. The scheduler permits at most two video jobs at once; image/GIF jobs may use the rest of the worker pool. Completion removes the job, decrements active-video accounting, wakes blocked workers, and sends the result to all waiters. A dropped receiver is ignored.

The processor runs inside a panic guard (`catch_unwind`): a panicking job produces a failed result instead of killing the worker loop, and every waiter receives a terminal outcome. Combined with the bounded queue — `MAX_PENDING_JOBS = 2048`; enqueueing beyond it fails fast with a "queue is full" error — no accepted job can silently disappear or grow memory without limit.

### Demand APIs and streaming limits

All demand paths first trust an existing database thumbnail path only when that file still exists. Otherwise they derive the current target from the asset's source version, reuse it if present, or schedule rendering. Every result is published through a compare-and-set write guarded by the source version: `update_asset_thumbnail_path_if_version_matches` (single) and `update_asset_thumbnail_paths_batch_versioned` (batch) update `assets.thumb_path` only when the record's path, size, and fingerprint still match the snapshot. A lost race reports `VersionMismatch`, and the caller removes the produced file so no stale-version target lingers on disk. Success clears the recorded failure for the asset version; a missing source or failed render clears the stored path and records a failure.

- `ensure_asset_thumbnail` is an internal, unregistered Rust helper with no frontend wrapper. It handles one asset at high priority and waits for its scheduler result with a 180-second budget (`DEMAND_JOB_TIMEOUT`); exceeding it returns a controlled timeout error instead of hanging. It returns `null` for an unknown asset, missing source, render failure, or a version lost to a concurrent re-index.
- `ensure_page_thumbnails` is also internal and unregistered. It de-duplicates the supplied IDs, clamps the batch to 256 unique IDs (`MAX_PAGE_BATCH`), and treats every found asset as high priority. Waiting uses a 180-second stall deadline. Its legacy `thumbnail-ready` emission is an implementation detail and is not part of the registered IPC contract.
- `ensure_thumbnails` is the production channel API. It retains positive, first-seen unique IDs, at most 64 from `visibleIds` and then at most 64 additional IDs from `prefetchIds`. Visible work is high priority and prefetch work is low priority, for a maximum of 128 assets.

The frontend sends its queue generation as `requestId`. The backend tracks the highest observed id; a request with a lower id belongs to an abandoned generation and is rejected immediately without channel events. Equal ids (sequential batches of one generation) proceed normally.

The stream sends `{ event: "ready" }` as each existing or generated path is found. After processing completes it sends one `{ event: "failed" }` for each failed ID, followed by one `{ event: "done" }` count. Ready events precede the final batch update of `assets.thumb_path`; the command result, not channel delivery, is the persistence boundary. Channel-send errors are ignored. See the [IPC contract](../architecture/ipc-contract.md) for the exact serialized union.

### Bulk render, retry, and cancellation

Only one `render_all_thumbnails` or `render_failed_thumbnails` operation may coordinate work at a time. Both submit to the shared scheduler and keep at most 24 scheduler results pending. Actual execution remains bounded by the scheduler worker count, the two-video limit, and the scheduler-wide 2048-job queue cap.

Render-all walks every asset in ID order. It counts an existing current target as generated, records a missing source as failed, and skips a current-version asset present in `thumbnail_failures`; this avoids repeatedly decoding known failures. Retry-failed first removes stale failure records, then selects only failures whose `asset_modified_at` equals the current asset `modified_at`, ordered oldest failure first. It attempts those jobs instead of skipping them. A success clears failure state. A failure upserts the row, increments `failure_count` only for the same version, records a generic or missing-source message, and updates `last_failed_at`.

The coordinator captures the thumbnail generation epoch when it starts and commits collected path updates through the versioned compare-and-set batch at the end. Waiting for results uses a stall deadline (`RESULT_STALL_TIMEOUT`, 180 seconds): if no completed result arrives within that window the command fails in a controlled way instead of hanging while holding its read lock.

Cancellation is cooperative. `cancel_render_all_thumbnails` returns `false` when no bulk coordinator is running; otherwise it sets an atomic flag and returns `true`. The coordinator checks before each enqueue and while waiting for results at 50 ms intervals. On cancellation it stops enqueueing/waiting, drains results already ready, batch-commits the collected path updates, emits the mode's done phase, and returns a summary with `cancelled: true` and possibly `processed < total`.

Scheduler jobs already queued or running are not removed or killed by bulk cancellation. They may still publish files after the bulk command returns, but results not ready during the nonblocking drain are not included in that bulk summary or database batch; each such result still publishes only through its own version guard. A later demand call discovers such a target on disk and reconciles its path.

### Generation epoch

`clear_all_thumbnails` bumps a process-wide `thumbnail_generation` counter before removing anything. Every demand/bulk coordinator captures the epoch at start and re-checks it before committing: when the epoch advanced meanwhile, none of the collected results are written to SQLite and all their produced files are removed, so a cleared library cannot be repopulated by renders that were already in flight.

### Frontend queue and tile updates

`useThumbnailQueue` maintains separate sets for queued and in-flight asset IDs plus a session-local failure set. `queueThumbnailsByIds` ignores non-positive IDs, IDs with a known or pending path, IDs failed during the current queue generation, and IDs already queued or in flight. A 36 ms timer coalesces requests. Processing is sequential by frontend batch, with up to 64 IDs per channel invocation; the backend scheduler provides parallel rendering inside that invocation.

As ready and failed messages arrive, the hook batches React state work behind a 24 ms UI flush. A ready path is merged without replacing the thumbnails object when nothing changed. Both ready and failed IDs leave the rendering state on flush. The pending count is the sum of queued and in-flight IDs; it drives the gallery's generation status.

`resetThumbnailQueue` cancels pending UI/process timers, increments the local generation, clears queued, in-flight, failed, pending-update, and rendering state, and clears the `ThumbnailStore`. It cannot cancel an invoke or backend scheduler work. Event callbacks and invoke-error handling compare their captured generation, so late work from before reset cannot repopulate frontend state. Reset also enables a previously failed ID to be queued again.

`ThumbnailStore` mirrors thumbnail paths and rendering IDs, but subscriptions are keyed by asset ID. Each changed asset gets a monotonically increasing in-memory version and only its listeners are notified. `GalleryTile` consumes that version with `useSyncExternalStore`, reads its effective path/rendering state directly, and can update a tile without rerendering the entire virtual grid. A tile shows the transparent placeholder on absence or image-load error and shows a spinner only while rendering with no ready path. GIF animation and other gallery presentation rules are covered in [library query and gallery](library-query-and-gallery.md).

### Locks, progress, cleanup, and database consistency

Generation, demand, bulk retry, and cancellation commands take the thumbnail read lock. `clear_all_thumbnails` takes the write lock, so it waits for active readers and excludes new ones while it clears. Root removal, asset delete/rename, database bundle operations, and library clearing use the combined scan lock then thumbnail write lock where they touch thumbnail ownership. The lock ordering and process isolation are defined in the [system overview](../architecture/system-overview.md).

All thumbnail commands run on Tauri's blocking thread pool (`spawn_blocking`): each command acquires its workflow lock inside the blocking task, so no lock is ever held across an await and no async runtime thread is occupied by scheduler waits or SQLite work. Workflow locks recover from poisoning (`into_inner`) instead of rejecting every later command after a panicked holder; the thumbnail processor's panic guard additionally keeps scheduler state unpoisoned in practice.

Every cleanup path treats `assets.thumb_path` as untrusted persisted data. Before deletion it canonicalizes the configured thumbnail root and candidate, rejects symlinks and non-files, and requires the canonical candidate to remain below the canonical thumbnail root. Missing, invalid, or outside-root paths count as unsuccessful best-effort cleanup and are never passed to `remove_file`.

Bulk runs emit start, mode-specific progress, warning, and done phases; progress is emitted every 25 processed assets and at completion. Page/stream demand emits `thumbs-page-start` and `thumbs-page-done`; due to the current double gate, intermediate `thumbs-page` events occur only at common multiples of 16 and 25 (every 400 items), plus completion. Cleanup reports every 25 paths and at completion. All use the shared best-effort `process-progress` broadcast; command/channel results remain authoritative.

Single demand updates the database through its version compare-and-set immediately after its result. Page/stream and bulk operations collect path updates and apply them in one SQLite transaction of per-row compare-and-set writes after processing or cancellation drain, while failure rows are written or cleared as individual results are handled. Failure-bookkeeping errors are deliberately ignored; a final path-update error rejects the command. A compare-and-set that loses against a re-index (or a generation reset) skips the write and best-effort deletes the produced file. Existing files can therefore temporarily precede their database references. Conversely, cleanup first clears database paths and failures, then best-effort removes the returned distinct files. A failed file removal leaves an unreferenced file; the reported removal count includes only successful deletions.

Asset/root deletion and rename clear related rows and best-effort remove known thumbnail files. Successful completed scan cleanup removes stale asset and failure rows, but ordinary rescans do not sweep unreferenced old-version targets from the thumbnail directory. Thumbnail path writes and clears do not bump the library revision because they do not change query membership or order.

## Known limitations

- Thumbnail identity covers path, size bytes, and nanosecond `fingerprint_mtime_ns` under cache version 2. File contents are hashed neither at scan time nor here: two different files with identical size and nanosecond mtime share one target. A render-algorithm or JPEG-setting change requires bumping `THUMB_CACHE_VERSION`; targets from older versions are regenerated on demand while old files linger as unreferenced orphans until an explicit cleanup.
- Bulk cancellation does not remove shared scheduler jobs. Work already queued can consume CPU and publish files after cancellation; each publication still passes its version guard, but only results ready at the drain are committed to the bulk summary/database batch.
- The scheduler has no shutdown protocol; workers live for the whole process lifetime.
- Video rendering is capped at two active jobs in code, but there is no focused test for that cap or for choosing a runnable image behind blocked videos. There are also no process-level tests for timeout killing, resource/PATH tool discovery, fallback rendering, or real ffmpeg output.
- Stream channel sends and progress broadcasts are best effort. The backend `requestId` staleness guard rejects abandoned generations' requests upfront but cannot cancel in-flight scheduler jobs, and rendering continues when the frontend drops the receiver.
- Frontend failures are sticky only until queue reset and are not surfaced per tile. There is no automatic backoff or retry in the production queue; reset or an explicit backend retry workflow is required.
- Temporary files left by process termination are not swept at startup. Failed best-effort deletion and rescan version changes can also leave orphan JPEGs, because cleanup is reference-driven rather than a directory reconciliation pass.
- Filesystem publication and SQLite updates are not one atomic transaction. A crash between rename and compare-and-set leaves a valid unreferenced target; a cleanup deletion failure can leave an unreferenced file; and a ready channel message may reach the UI before the final path batch commits.
- Failure-record and failure-clear database errors are swallowed, so a successful render result does not strictly guarantee that its retry marker was cleared, and a failed render does not strictly guarantee that its retry marker was persisted.
- Failure rows are guarded only by seconds-resolution `asset_modified_at`, so a same-second re-index can leave a failure marker attributed to the superseded record until stale-row cleanup removes it.
- The page-progress double gate makes intermediate progress extremely sparse. This is current behavior, not an intended sampling recommendation.

## Safe change checklist

1. Preserve one target identity across every entry point. The key already carries a cache version (`THUMB_CACHE_VERSION`); if identity inputs or rendering rules change, bump it and plan regeneration/cleanup of orphaned targets; test same-path versions, same-second modifications, and old database references.
2. Keep temporary writes and final publication in the same directory. Test decode/render failure, stale temporary files, an already-present final target, and concurrent waiters without exposing a partial JPEG.
3. For image changes, test the 390-pixel bound, aspect preservation, exact `1.6` crop threshold, 96-pixel top bias and clamp, GIF decoding, RGB/JPEG conversion, and small/zero-dimension edge behavior.
4. For video changes, test every seek boundary, unknown duration, zero-second fallback, scale/output arguments, nonzero exit, missing executables, probe parsing, 12/45-second timeouts, and child termination.
5. Keep ffmpeg and ffprobe discovery aligned. Exercise resource, executable-adjacent, `/usr/bin`, and system `PATH` layouts.
6. Preserve scheduler target de-duplication, waiter fan-out, low-to-high promotion, FIFO priority, image progress around blocked videos, worker bounds, the two-video cap, panic-safe terminal results, and the `MAX_PENDING_JOBS` queue bound. Startup must keep refusing to run when worker threads fail to spawn.
7. Keep the Rust stream event enum, TypeScript union, API wrapper, command registration, positive-ID filtering, exact 64-visible/64-prefetch caps, priority split, and one terminal `done` synchronized. Follow the [IPC contract](../architecture/ipc-contract.md) for any transport change.
8. Preserve frontend 64-item batching, 36 ms queue coalescing, 24 ms UI flushing, queued/in-flight de-duplication, failure suppression, generation checks, reset semantics, and per-asset store notification. Test rejected invokes, messages after reset, unmount, and overlapping queue additions.
9. Treat cancellation as a partial commit. Test cancellation before enqueue, while below and at the 24-result window, during final drain, with image/video jobs still running, and verify summary counts, failure rows, published files, and subsequent reconciliation.
10. Keep lock order consistent for generation, clear, rename/delete, scan-root removal, library clear, and bundle replacement. Test that exclusive cleanup cannot race a demand reader and that best-effort filesystem failure leaves truthful counts and recoverable database state.
11. When changing failure policy, test count reset on asset-version change, stale-row cleanup, render-all skip, retry-failed ordering, success clearing, missing sources, and repeated failures. Keep seconds-versus-nanoseconds behavior explicit until the schema and target key migrate together.
12. Run Rust formatting and focused `thumbs`, scheduler, service, database, and command tests; run the frontend queue and gallery tests; then exercise a packaged desktop build with real image, GIF, short/long video, bundled tools, system tools, cancellation, and cleanup.

### Relevant existing tests

- `src-tauri/src/thumbs.rs` covers tall-image crop geometry, duration parsing, video seek boundaries, version-sensitive target hashing, and decode-budget boundaries.
- `src-tauri/src/services/thumb_scheduler.rs` covers in-flight target de-duplication, waiter fan-out, high-priority ordering, parallel execution, panic-safe terminal results, version echo in results, and the queue-full bound.
- `src-tauri/src/services/thumb_service.rs` covers stall timeouts, cancellation flag behavior, reuse of an existing thumbnail, file-removal counts, the scan-update-versus-completion race (stale render dropped and its file removed), and generation-reset publication drops.
- `src-tauri/src/commands/thumbs.rs` covers the cancellation service's accepted/not-running states behind the async command adapter.
- `src-tauri/src/db.rs` covers thumbnail compare-and-set outcomes (applied, no-op repeat, re-index mismatch), stale-ID reporting for batch writes, same-second rescan reference release, GIF inclusion in bulk candidates, versioned failure filtering/counting, and failure cleanup on asset deletion.
- `src/hooks/__tests__/useThumbnailQueue.test.ts` covers queue de-duplication, failure suppression, streamed ready merging, stale-generation rejection, and 64-item frontend batches.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx` covers rendering indicators and gallery-level thumbnail status among the virtual-grid behaviors.

These tests do not currently provide an end-to-end thumbnail render through a real decoder/ffmpeg process or assert channel serialization through a desktop WebView; the cleanup and cancellation race cases are covered at unit level only.
