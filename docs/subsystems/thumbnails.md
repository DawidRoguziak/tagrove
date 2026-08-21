# Thumbnails

This subsystem creates versioned JPEG previews, schedules demand and bulk work, persists paths and failures, and delivers per-asset updates to the virtual gallery. The implementation in `src-tauri/src/thumbs.rs`, `src-tauri/src/services/thumb_scheduler.rs`, `src-tauri/src/services/thumb_service.rs`, the thumbnail database functions, `src/hooks/useThumbnailQueue.ts`, and their executable tests is the source of truth.

Command names, payload shapes, channel serialization, and shared progress-event rules live in the [IPC contract](../architecture/ipc-contract.md). Database schema and transaction conventions live in [database persistence](database.md), scan-time metadata and fingerprints in [scanning and indexing](scanning-and-indexing.md), gallery range loading and presentation in [library query and gallery](library-query-and-gallery.md), and app-data, resource, and lock lifecycles in the [system overview](../architecture/system-overview.md).

## Current guarantees

### Cache and target identity

The thumbnail directory is `<app-data>/thumbs` and is created at startup. `thumb_target` hashes these bytes with SHA-256:

1. the source path's lossy UTF-8 representation; and
2. the asset's signed 64-bit `modified_at`, encoded little-endian.

The target is the lowercase hexadecimal digest plus `.jpg`. Asset ID, media kind, render settings, and file contents are not part of the key. The path makes one source version converge on one target across single-item, page, stream, and bulk entry points. A target already present on disk is treated as ready and its path is reconciled into SQLite.

`modified_at` is Unix time in whole seconds. The scanner separately stores and compares `fingerprint_mtime_ns` at nanosecond resolution to decide whether an asset changed. This unit split is an important versioning constraint; see Known limitations.

### Image and GIF rendering

Images and GIFs use the Rust `image` decoder and produce an RGB JPEG. The maximum normal thumbnail box is 390 by 390 pixels with Lanczos3 filtering. A portrait with `height / width >= 1.6` is handled specially: a square as wide as the source is cropped from `x = 0`, with its top at `min(96, height - width)`, and resized exactly to 390 by 390. Other images use aspect-preserving `thumbnail(390, 390)` behavior. The gallery applies `object-cover` when presenting the result.

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

Both probing and rendering poll child completion every 25 ms. A timed-out child is killed and waited on. On Windows, `ffmpeg` and `ffprobe` are launched with `CREATE_NO_WINDOW`, so scans and thumbnail generation do not flash console windows.

### ffmpeg and ffprobe discovery

The Tauri bundle declares `binaries/ffmpeg` and `binaries/ffprobe` as external binaries. At startup, ffmpeg discovery searches the resource directory and its `binaries` child, then equivalent executable-adjacent and `resources` locations. In each directory it considers `ffmpeg.exe`, `ffmpeg`, and the lexically first file whose stem starts with `ffmpeg-`. The first existing candidate wins; otherwise the scheduler stores bare `ffmpeg` for operating-system `PATH` lookup.

For a probe, the code derives an ffprobe name by replacing an `ffmpeg` filename prefix while retaining its suffix, then tries sibling `ffprobe.exe`, sibling `ffprobe`, and bare `ffprobe`, with duplicate paths removed. If those fail, the ffmpeg stderr fallback uses the already-resolved ffmpeg path. A missing or unlaunchable video tool therefore becomes an absent duration and, later, a thumbnail failure rather than preventing application startup.

### Shared scheduler

One process-wide `ThumbnailScheduler` is created at startup. Its worker count is `available_parallelism - 2`, clamped to 2 through 8, or 4 when parallelism cannot be read. A constructor still enforces at least one worker for tests and direct callers.

The scheduler has FIFO high- and low-priority queues. Workers search the high queue first, but may pass a blocked video and run another eligible job. Single-item demand, legacy page demand, and visible streamed IDs are high priority; streamed prefetch and bulk work are low priority. A later high-priority waiter promotes a queued low-priority target. Work already running cannot be preempted.

Jobs are keyed by the target path string. Enqueuing an existing target job attaches a waiter rather than starting another render. Each waiter retains its own asset ID and receives the shared path-or-failure result. This de-duplicates work across entry points while allowing every caller to update its own state. The scheduler permits at most two video jobs at once; image/GIF jobs may use the rest of the worker pool. Completion removes the job, decrements active-video accounting, wakes blocked workers, and sends the result to all waiters. A dropped receiver is ignored.

### Demand APIs and streaming limits

All demand paths first trust an existing database thumbnail path only when that file still exists. Otherwise they derive the current target, reuse it if present, or schedule rendering. Success clears the recorded failure for the asset version; a missing source or failed render clears the stored path and records a failure.

- `ensure_asset_thumbnail` handles one asset at high priority and blocks until its scheduler result. It returns `null` for an unknown asset, missing source, or render failure.
- `ensure_page_thumbnails` de-duplicates the supplied IDs without filtering non-positive values or imposing a batch cap. Every found asset is high priority. It returns `ready` items and `failed` IDs, and emits the legacy `thumbnail-ready` application event for every ready item.
- `ensure_thumbnails` is the production channel API. It retains positive, first-seen unique IDs, at most 64 from `visibleIds` and then at most 64 additional IDs from `prefetchIds`. Visible work is high priority and prefetch work is low priority, for a maximum of 128 assets. The Rust command accepts but ignores `requestId`.

The stream sends `{ event: "ready" }` as each existing or generated path is found. After processing completes it sends one `{ event: "failed" }` for each failed ID, followed by one `{ event: "done" }` count. Ready events precede the final batch update of `assets.thumb_path`; the command result, not channel delivery, is the persistence boundary. Channel-send errors and legacy event-delivery errors are ignored. See the [IPC contract](../architecture/ipc-contract.md) for the exact serialized union.

### Bulk render, retry, and cancellation

Only one `render_all_thumbnails` or `render_failed_thumbnails` operation may coordinate work at a time. Both submit to the shared scheduler and keep at most 24 scheduler results pending. Actual execution remains bounded by the scheduler worker count and the two-video limit.

Render-all walks every asset in ID order. It counts an existing current target as generated, records a missing source as failed, and skips a current-version asset present in `thumbnail_failures`; this avoids repeatedly decoding known failures. Retry-failed first removes stale failure records, then selects only failures whose `asset_modified_at` equals the current asset `modified_at`, ordered oldest failure first. It attempts those jobs instead of skipping them. A success clears failure state. A failure upserts the row, increments `failure_count` only for the same version, records a generic or missing-source message, and updates `last_failed_at`.

Cancellation is cooperative. `cancel_render_all_thumbnails` returns `false` when no bulk coordinator is running; otherwise it sets an atomic flag and returns `true`. The coordinator checks before each enqueue and while waiting for results at 50 ms intervals. On cancellation it stops enqueueing/waiting, drains results already ready, batch-commits the collected path updates, emits the mode's done phase, and returns a summary with `cancelled: true` and possibly `processed < total`.

Scheduler jobs already queued or running are not removed or killed by bulk cancellation. They may still publish files after the bulk command returns, but results not ready during the nonblocking drain are not included in that bulk summary or database batch. A later demand call discovers such a target on disk and reconciles its path.

### Frontend queue and tile updates

`useThumbnailQueue` maintains separate sets for queued and in-flight asset IDs plus a session-local failure set. `queueThumbnailsByIds` ignores non-positive IDs, IDs with a known or pending path, IDs failed during the current queue generation, and IDs already queued or in flight. A 36 ms timer coalesces requests. Processing is sequential by frontend batch, with up to 64 IDs per channel invocation; the backend scheduler provides parallel rendering inside that invocation.

As ready and failed messages arrive, the hook batches React state work behind a 24 ms UI flush. A ready path is merged without replacing the thumbnails object when nothing changed. Both ready and failed IDs leave the rendering state on flush. The pending count is the sum of queued and in-flight IDs; it drives the gallery's generation status.

`resetThumbnailQueue` cancels pending UI/process timers, increments the local generation, clears queued, in-flight, failed, pending-update, and rendering state, and clears the `ThumbnailStore`. It cannot cancel an invoke or backend scheduler work. Event callbacks and invoke-error handling compare their captured generation, so late work from before reset cannot repopulate frontend state. Reset also enables a previously failed ID to be queued again.

`ThumbnailStore` mirrors thumbnail paths and rendering IDs, but subscriptions are keyed by asset ID. Each changed asset gets a monotonically increasing in-memory version and only its listeners are notified. `GalleryTile` consumes that version with `useSyncExternalStore`, reads its effective path/rendering state directly, and can update a tile without rerendering the entire virtual grid. A tile shows the transparent placeholder on absence or image-load error and shows a spinner only while rendering with no ready path. GIF animation and other gallery presentation rules are covered in [library query and gallery](library-query-and-gallery.md).

### Locks, progress, cleanup, and database consistency

Generation, demand, bulk retry, and cancellation commands take the thumbnail read lock. `clear_all_thumbnails` takes the write lock, so it waits for active readers and excludes new ones while it clears. Root removal, asset delete/rename, database bundle operations, and library clearing use the combined scan lock then thumbnail write lock where they touch thumbnail ownership. The lock ordering and process isolation are defined in the [system overview](../architecture/system-overview.md).

Every cleanup path treats `assets.thumb_path` as untrusted persisted data. Before deletion it canonicalizes the configured thumbnail root and candidate, rejects symlinks and non-files, and requires the canonical candidate to remain below the canonical thumbnail root. Missing, invalid, or outside-root paths count as unsuccessful best-effort cleanup and are never passed to `remove_file`.

Bulk runs emit start, mode-specific progress, warning, and done phases; progress is emitted every 25 processed assets and at completion. Page/stream demand emits `thumbs-page-start` and `thumbs-page-done`; due to the current double gate, intermediate `thumbs-page` events occur only at common multiples of 16 and 25 (every 400 items), plus completion. Cleanup reports every 25 paths and at completion. All use the shared best-effort `process-progress` broadcast; command/channel results remain authoritative.

Single demand updates the database after its result. Page/stream and bulk operations collect path updates and apply them in one SQLite transaction after processing or cancellation drain, while failure rows are written or cleared as individual results are handled. Failure-bookkeeping errors are deliberately ignored; a final path-update error rejects the command. Existing files can therefore temporarily precede their database references. Conversely, cleanup first clears database paths and failures, then best-effort removes the returned distinct files. A failed file removal leaves an unreferenced file; the reported removal count includes only successful deletions.

Asset/root deletion and rename clear related rows and best-effort remove known thumbnail files. Successful completed scan cleanup removes stale asset and failure rows, but ordinary rescans do not sweep unreferenced old-version targets from the thumbnail directory. Thumbnail path writes and clears do not bump the library revision because they do not change query membership or order.

## Known limitations

- Thumbnail identity uses seconds-resolution `modified_at`, while incremental scan detection uses nanosecond `fingerprint_mtime_ns`. If a file changes within the same second, scanning can re-index it but preserve the old `thumb_path`, and `thumb_target` still names the same cache file. Size, content hash, nanosecond mtime, render algorithm version, and settings are absent from the key, so this can serve a stale thumbnail until it is explicitly cleared.
- A render algorithm or JPEG-setting change does not invalidate existing targets. There is no cache schema/version salt or automated rebuild migration.
- Bulk cancellation does not remove shared scheduler jobs. Work already queued can consume CPU and publish files after cancellation; only results ready at the drain are committed to the bulk summary/database batch.
- The scheduler has no shutdown protocol or bounded queue, and its worker thread spawn results are ignored. Demand receivers wait without a service-level timeout; only ffmpeg/ffprobe child processes have timeouts.
- Video rendering is capped at two active jobs in code, but there is no focused test for that cap or for choosing a runnable image behind blocked videos. There are also no process-level tests for timeout killing, Windows hidden-process flags, bundled/PATH tool discovery, fallback rendering, or real ffmpeg output.
- Page demand has no backend input cap, and all page tasks are enqueued before results are consumed. The production frontend stays at 64, but another IPC caller can create a much larger pending set.
- Stream channel sends and progress broadcasts are best effort. The ignored backend `requestId` cannot correlate or cancel work, and the backend continues when the frontend resets or drops the receiver.
- Frontend failures are sticky only until queue reset and are not surfaced per tile. There is no automatic backoff or retry in the production queue; reset or an explicit backend retry workflow is required.
- Temporary files left by process termination are not swept at startup. Failed best-effort deletion and rescan version changes can also leave orphan JPEGs, because cleanup is reference-driven rather than a directory reconciliation pass.
- Filesystem publication and SQLite updates are not one atomic transaction. A crash or database error can leave a valid unreferenced target; a cleanup deletion failure can leave an unreferenced file; and a ready channel message may reach the UI before the final path batch commits.
- Failure-record and failure-clear database errors are swallowed, so a successful render result does not strictly guarantee that its retry marker was cleared, and a failed render does not strictly guarantee that its retry marker was persisted.
- The page-progress double gate makes intermediate progress extremely sparse. This is current behavior, not an intended sampling recommendation.

## Safe change checklist

1. Preserve one target identity across every entry point. If identity inputs or rendering rules change, add an explicit cache version and a migration/cleanup plan; test same-path versions, same-second modifications, and old database references.
2. Keep temporary writes and final publication in the same directory. Test decode/render failure, stale temporary files, an already-present final target, and concurrent waiters without exposing a partial JPEG.
3. For image changes, test the 390-pixel bound, aspect preservation, exact `1.6` crop threshold, 96-pixel top bias and clamp, GIF decoding, RGB/JPEG conversion, and small/zero-dimension edge behavior.
4. For video changes, test every seek boundary, unknown duration, zero-second fallback, scale/output arguments, nonzero exit, missing executables, probe parsing, 12/45-second timeouts, child termination, and Windows `CREATE_NO_WINDOW` behavior.
5. Keep bundled ffmpeg and ffprobe configuration aligned with startup and sibling discovery. Exercise resource, sidecar, executable-adjacent, and system `PATH` layouts on supported packaging targets.
6. Preserve scheduler target de-duplication, waiter fan-out, low-to-high promotion, FIFO priority, image progress around blocked videos, worker bounds, and the two-video cap. Add a shutdown or queue bound before relying on either property operationally.
7. Keep the Rust stream event enum, TypeScript union, API wrapper, command registration, positive-ID filtering, exact 64-visible/64-prefetch caps, priority split, and one terminal `done` synchronized. Follow the [IPC contract](../architecture/ipc-contract.md) for any transport change.
8. Preserve frontend 64-item batching, 36 ms queue coalescing, 24 ms UI flushing, queued/in-flight de-duplication, failure suppression, generation checks, reset semantics, and per-asset store notification. Test rejected invokes, messages after reset, unmount, and overlapping queue additions.
9. Treat cancellation as a partial commit. Test cancellation before enqueue, while below and at the 24-result window, during final drain, with image/video jobs still running, and verify summary counts, failure rows, published files, and subsequent reconciliation.
10. Keep lock order consistent for generation, clear, rename/delete, scan-root removal, library clear, and bundle replacement. Test that exclusive cleanup cannot race a demand reader and that best-effort filesystem failure leaves truthful counts and recoverable database state.
11. When changing failure policy, test count reset on asset-version change, stale-row cleanup, render-all skip, retry-failed ordering, success clearing, missing sources, and repeated failures. Keep seconds-versus-nanoseconds behavior explicit until the schema and target key migrate together.
12. Run Rust formatting and focused `thumbs`, scheduler, service, database, and command tests; run the frontend queue and gallery tests; then exercise a packaged desktop build with real image, GIF, short/long video, bundled tools, system tools, cancellation, and cleanup.

### Relevant existing tests

- `src-tauri/src/thumbs.rs` covers tall-image crop geometry, duration parsing, and video seek boundaries.
- `src-tauri/src/services/thumb_scheduler.rs` covers in-flight target de-duplication, waiter fan-out, high-priority ordering, and parallel execution.
- `src-tauri/src/services/thumb_service.rs` covers result waiting, cancellation flag behavior, reuse of an existing thumbnail, and file-removal counts.
- `src-tauri/src/commands/thumbs.rs` covers the cancellation command's accepted/not-running states.
- `src-tauri/src/db.rs` covers preservation of a thumbnail for unchanged seconds-resolution time, GIF inclusion in bulk candidates, versioned failure filtering/counting, and failure cleanup on asset deletion.
- `src/hooks/__tests__/useThumbnailQueue.test.ts` covers queue de-duplication, failure suppression, streamed ready merging, stale-generation rejection, and 64-item frontend batches.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx` covers rendering indicators and gallery-level thumbnail status among the virtual-grid behaviors.

These tests do not currently provide an end-to-end thumbnail render through a real decoder/ffmpeg process, assert channel serialization through a desktop WebView, or cover the cleanup and cancellation race cases listed above.
