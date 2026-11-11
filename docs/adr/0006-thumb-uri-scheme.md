# 0006: Gallery thumbnails through a `thumb://` URI scheme

- Status: Accepted
- Date: 2026-11-07
- Areas: thumbnails, IPC, gallery, security, dependencies
- Related: [thumbnails](../subsystems/thumbnails.md), [IPC contract](../architecture/ipc-contract.md), [system overview](../architecture/system-overview.md), [library query and gallery](../subsystems/library-query-and-gallery.md), [plan stage 06](../plans/grid-performance/06-thumb-protocol.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md), [ADR 0003](0003-custom-gallery-virtualizer.md), [ADR 0024](0024-thumbnail-retries-continue-while-the-tile-is-mounted.md) (retries after the placeholder)

## Context

Gallery thumbnails reached tiles through a frontend queue (`createThumbnailQueue`, `thumbnailStore`, `createAssetThumbnail`) and one channel command, `ensure_thumbnails`. The queue coalesced IDs for 36 ms, sent at most 64 per call, waited for the whole batch, and flushed results to a per-asset store every 24 ms. The backend high-priority queue was FIFO and ignored the request ID, so after a jump the new viewport waited behind renders for tiles that were already gone. One slow video could hold a batch for up to two 45-second ffmpeg attempts. Summaries carried an absolute `thumb_path`, which tiles turned into asset-protocol URLs.

Measured before this change (plan stage 07, step 0): empty cache, debug build, per-tile thumbnail p50 354 ms, p95 444 ms; a synthetic 24 MP JPEG took 98 ms to render in release.

## Options considered

1. **Keep the queue and fix it (plan B):** per-item results, a real request ID with cancellation, LIFO. Keeps three frontend modules, timers and a store that mirrors what the browser image cache already does.
2. **Own URI scheme addressed by asset ID (chosen):** tiles set `src` and WebKit requests, caches and decodes. The backend resolves the file from the database and renders on demand.
3. **JPEG decoders:**
   - `jpeg-decoder` with IDCT scaling (chosen);
   - no new dependency (`image`/`zune-jpeg` plus a cheaper resize);
   - `turbojpeg`/libjpeg-turbo, rejected because it is a C parser for untrusted user files, with a history of memory-safety CVEs, and a system dependency that complicates Flatpak and AppImage;
   - `ffmpeg -lowres`, rejected because a process per image costs more than it saves and widens the C attack surface from videos to every image.

## Decision

- `thumb://localhost/<id>?v=<thumb_version>[&c=1][&p=1][&r=<n>]` is registered with `register_asynchronous_uri_scheme_protocol` (`services/thumb_protocol.rs`). Only a positive numeric ID, a 16-hex version, `c=1`, `p=1` and a retry counter are accepted; there are no paths. `thumbnailUrl` in `src/platform/tauri/thumbnails.ts` is the only URL builder.
- `AssetSummary.thumb_path` is replaced by `thumb_version`, the first 16 hex characters of the existing cache key (a prefix of the target file stem). `DuplicateAsset` carries it too. `AssetDetails` keeps `thumb_path`.
- An existing file is read and served within one short `thumb_read_section`. A missing one from a visible tile is enqueued at a new `Interactive` priority, which is LIFO and ahead of High/Low; an overscan tile sends `p=1` and is enqueued at High (FIFO). The result arrives through a scheduler callback (`enqueue_with_callback`), so no thread waits for a render. A publisher commits all results already waiting (at most 64) in one transaction, keeping the compare-and-set and generation rules of ADR 0002.
- `200` responses are `immutable` when the served version matches `v`; every other response is `no-store`. CORS echoes `tauri://localhost` or the Vite dev origin.
- A render that failed in this process answers `404` without retrying; a failure marker from an earlier run gets one on-demand retry. `c=1` serves only existing files and is used for assets first shown during a fast-scroll pause.
- The frontend queue, store, per-asset bindings, `ensure_thumbnails`, `ThumbnailStreamEvent`, the unregistered single-asset/page helpers and the "generating thumbnails" footer counter are removed. `ThumbnailImage` retries a failed load twice (500 ms, 2 s) on the same element. The tile spinner is CSS only.
- `$APPDATA/thumbs/**` is removed from the asset-protocol scope; the remaining `**` serves lightbox originals and is a separate security topic.
- JPEG sources are decoded with `jpeg-decoder` 0.3 at the smallest IDCT scale that covers the thumbnail. It is built with `platform_independent` and without default features, so it contains no `unsafe` code and no rayon pool. Other formats are decoded once through `image` instead of probe-then-open. The tall-image crop resizes with Triangle instead of Lanczos3.
- Video thumbnails try `-skip_frame nokey -noaccurate_seek` first, then the previous exact seek, then seek 0.

## Consequences

- **Spike results.**
  - WebKitGTK/wry 0.55 exposes no cancellation for custom-scheme requests: the handler gets a one-shot responder, so every request is answered. LIFO plus the cached-only pause replaces cancellation. No `thumb_demote` command was added.
  - The origin is `tauri://localhost`, or `http://localhost:1420` in development; images with `crossorigin="anonymous"` load once the header echoes it. Desktop E2E (`gallery-browsing`) checks decoded `naturalWidth` through the real scheme.
  - The handler runs on the GTK main thread and only parses before handing work to the blocking pool.
- **JPEG decoding:** the expected 2x was not reached. Huffman entropy decoding dominates and runs at full resolution at every IDCT scale:

  | Measurement (release, synthetic 24 MP JPEG) | Time |
  | --- | --- |
  | `jpeg-decoder` scaled 1/8, decode only | 74 ms |
  | `zune-jpeg` full, decode only | 72 ms |
  | Thumbnail end to end, before | 98 ms |
  | Thumbnail end to end, after | 78 ms (−20%) |

  The gain comes from not resizing a 24 MP buffer. The user accepted the dependency for this −20% after the measurement. A no-dependency variant measured about 85–93 ms.
- **Video:** a synthetic 1080p H.264 clip with 250-frame GOPs went from 202 ms to 48 ms per thumbnail. The frame is the keyframe at or before the seek point instead of the exact time. Some inputs give no frame in keyframe mode, for example a 0.5 s single-keyframe clip; they pay one extra short ffmpeg run before the exact attempt.
- **Cache version:** `THUMB_CACHE_VERSION` was not bumped. Scaled decoding, the Triangle crop filter and keyframe video frames produce equivalent pictures of the same source version, and a bump would re-render every library. Existing thumbnails stay valid; new ones may differ by resampling detail or by a few seconds of video.
- **Contract:** the IPC change is non-additive. `ensure_thumbnails` is gone, and `AssetSummary.thumb_path` became `thumb_version`.
- **Lifetime:** tile URLs are stable per source version, so refresh, eviction, restore and root removal no longer need thumbnail resets. WebKit's image cache holds the decoded pictures.
- **Tests:**
  - Rust: parser, headers, LIFO, callbacks, failure memory, cached-only mode, scaled-decode geometry, keyframe video;
  - Vitest: `ThumbnailImage` retries, tile URLs and the pause rule, the `thumbnailUrl` shape;
  - desktop E2E: real loading.
- **Follow-up:** a second size for HiDPI tiles (deferred by decision); narrowing the asset-protocol `**` scope.
