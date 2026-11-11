# 0020: Image thumbnails apply EXIF orientation, and cache version 3 adopts a legacy thumbnail only when the picture is unchanged

- Status: Accepted
- Date: 2026-11-11
- Areas: thumbnails (rendering, cache identity, `thumb://` demand path, Render all)
- Related: [thumbnails](../subsystems/thumbnails.md#image-and-gif-rendering), [ADR 0006](0006-thumb-uri-scheme.md), [ADR 0018](0018-render-all-keeps-thumbnails-of-unreachable-sources.md), [ADR 0019](0019-clear-epoch-keeps-current-version-targets.md)

## Context

`create_image_thumb` (`src-tauri/src/thumbs.rs`) decoded through `jpeg-decoder` scanlines or `image::DynamicImage::from_decoder`. Neither applies the EXIF orientation, and the crop decision ran on the stored size. A phone portrait stored as landscape pixels with Orientation 6 or 8 therefore got a sideways tile, and a rotated tall image got the landscape path instead of the top-biased square crop, while the lightbox shows the original upright (WebKit applies `image-orientation: from-image` by default).

Fixing the renderer changes the picture for every rotated or mirrored image, so the cache version must change. The demand path served any recorded `thumb_path` that still existed, and the thumbnails page said that a version bump alone does not invalidate those references. Old sideways thumbnails would then stay for good, and labelled with the new `thumb_version` they would also be cached by WebKit as `immutable`. Most recorded thumbnails (no EXIF transform, all videos and GIFs) are still exactly what the new renderer would draw.

## Options considered

1. **Bump the version and keep serving recorded files.** No cost, but the fix never reaches an existing library unless the user runs Render all, which redraws every asset, videos included, and leaves every old file orphaned.
2. **Release every reference at startup** (one-time pass keyed by a `library_metadata` marker). Correct, but the whole library becomes a cold cache again: every image and video is decoded once more, and the old files still need a sweep.
3. **Read the source orientation on the `thumb://` request thread** and serve or redraw from there. This puts source I/O inside the thumbnail read section on the hot path. An unreachable mount would stall it.
4. **Hand a legacy reference to the render worker.** The worker reads only the source header. When the new rules draw the same picture, it moves the recorded file to the current target. Otherwise it renders and removes the superseded file.

## Decision

Option 4.

- Both decode paths read the orientation before the crop decision. The scaled JPEG path reads it from the APP1 segment that `read_info` parses before the frame header, picks the IDCT scale for the displayed size (axes swapped for quarter turns), and rotates the scaled buffer. The general path uses `ImageDecoder::orientation` and, for a transform, shrinks to the same request before rotating, so a large source is not copied at full size. Unreadable orientation metadata means no transform. `THUMB_CACHE_VERSION` is 3.
- A recorded `thumb_path` that is not the current target is a legacy reference. `request_thumbnail` never serves it from the read section and does not check its source there. With `c=1` it answers `404`, so that no old picture is cached under the new URL. Otherwise, when the file is owned (a regular file inside the canonical thumbnail root), the request enqueues a render whose task carries the reference as `legacy_thumb`, without the `SOURCE_MISSING` check. Render all passes the same field when the current target is missing.
- The default processor (`process_thumbnail_task`) checks the source outside the read section, and checks again that the legacy file is a regular file inside the target's canonical directory before it serves, moves or removes it. For an unreachable source (`Path::exists` is false) it returns the legacy path, so the recorded thumbnail stays in use as before ([ADR 0018](0018-render-all-keeps-thumbnails-of-unreachable-sources.md)); a demand result that keeps the recorded path of a row without a failure marker is served without a write. Any served file that is not the current target gets an empty version token, so `image_response` sends `no-store` and the old picture is never cached under the new URL. Otherwise it renames the legacy file to the target when the asset is a video or when `thumbs::image_orientation` reports no transform or cannot read the header. After a fresh render it removes the legacy file. Publication then goes through the usual compare-and-set. A video job with `legacy_thumb` is not counted against the browsing cap of two video jobs, so adoptions do not queue behind ffmpeg renders; one whose rename fails renders outside the cap.

## Consequences

- An existing library is corrected on demand. Unrotated images, GIFs and videos cost one header read (no read for a video) and a rename. Rotated and mirrored images are rendered again. Render all after the upgrade also redraws only those images.
- While its source stays unreachable, an asset with a legacy file goes through a worker on each uncached request: one stat and a file read, no decode and no database write. The source stat never runs inside the thumbnail read section, so a hung network mount occupies workers, as any render from that mount would, but never blocks the guard that Clear all, deletes and scans wait for.
- Between the rename and the commit, the row still points at the old path. A concurrent request finds the old path missing and serves the target that is already on disk, and a crash in between leaves a valid target that the next request reconciles. Workers keep their operation permit, so backup and restore never see that window.
- The probe sees only EXIF placed before the frame header, as the Exif standard requires. A non-conforming JPEG with APP1 after SOF keeps its legacy picture unrotated.
- A legacy file whose redraw fails stays on disk unreferenced (the failure write clears `thumb_path`) until Clear all. A legacy reference that is not an owned file is rendered anew and its file is never touched.
- No schema, IPC or library-revision change. The frontend is unchanged, because `thumb_version` already follows the cache key.
- Tests: `thumbs.rs` unit tests cover synthetic JPEGs and PNGs with Orientation absent, 1, 2, 3, 5, 6, 8 and an invalid 9 on both decode paths (dimensions and corner colours), the crop decided on the displayed size, and the swapped decode request. `thumb_service.rs` tests cover adoption without rendering, a rotated image rendered again, Render all over images, a mirrored image and a video, a cached-only miss, and an unreachable source that still serves its recorded file through the worker without changing the row and with `no-store`. A scheduler test covers a video adoption that runs while both browsing video slots are busy (`thumb_scheduler.rs`). The existing test that serves a recorded file while another connection holds the SQLite write lock now covers the no-write path for a legacy file.
