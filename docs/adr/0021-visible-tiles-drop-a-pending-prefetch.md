# 0021: A tile that scrolls into view before its prefetched thumbnail arrives drops `p=1`, and High stays FIFO

- Status: Accepted
- Date: 2026-11-11
- Areas: thumbnails (scheduler order, `thumb://` demand), gallery tiles
- Related: [thumbnails](../subsystems/thumbnails.md#frontend-tiles), [library query and gallery](../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch), [ADR 0003](0003-custom-gallery-virtualizer.md), [ADR 0006](0006-thumb-uri-scheme.md)

## Context

[ADR 0006](0006-thumb-uri-scheme.md) put visible `thumb://` requests at `Interactive` (LIFO) and overscan requests (`p=1`) at `High` (FIFO), behind every Interactive job. `GalleryTile` froze the prefetch flag for an asset when the tile first showed it, so its URL never changed while it stayed in that tile.

During a steady scroll below the fast-scroll pause threshold, a recycled row slot receives its new asset while the row is in the four-row overscan band (`GalleryGrid.tsx`, `prefetch` = outside the visible index range). Nearly every tile was therefore requested once at High and never again: WebKit does not request an unchanged `src` again, and the scheduler promotes a job only on a repeated enqueue (`thumb_scheduler.rs`, `stronger_priority`). Interactive stayed almost empty. Once rendering fell behind the scroll, the tiles visible where the user stopped were the newest entries of a FIFO backlog and rendered after every tile already scrolled past. WebKitGTK gives custom-scheme handlers no cancellation, so those renders cannot be dropped (ADR 0006). The audit found this as `S-389855fd` (audit 20261108-1145); its QA run on a fast host did not reproduce a visible wait.

Measured with `steady_scroll_visible_thumbnail_wait_baseline` (`src-tauri/tests/grid_baseline.rs`, release, 16-CPU host). The model has 8 columns, 5 visible rows and 4 overscan rows, and scrolls 160 rows at 20 rows/s over never-rendered images (a synthetic 24 MP JPEG, 61.5 ms per render), then stops. The real scheduler renders, with 8 workers (the browsing count of this host) and with 2 (the browsing minimum, as on a 4-CPU host). The table shows how long the 40 tiles visible at the stop still wait for their result:

| Workers | Requests | Backlog at stop | Ready at stop | Wait p50 | Wait p95 | Wait max | Wait mean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 8 | sticky `p=1` (before) | 339 | 0/40 | 2,264 ms | 2,403 ms | 2,441 ms | 2,271 ms |
| 8 | LIFO High (option 1) | 336 | 32/40 | 0 ms | 207 ms | 223 ms | 41 ms |
| 8 | `p=1` dropped in view (option 2) | 337 | 16/40 | 28 ms | 157 ms | 159 ms | 51 ms |
| 2 | sticky `p=1` (before) | 1,093 | 0/40 | 32.0 s | 32.5 s | 32.6 s | 32.0 s |
| 2 | LIFO High (option 1) | 1,092 | 8/40 | 1,291 ms | 1,787 ms | 1,848 ms | 1,109 ms |
| 2 | `p=1` dropped in view (option 2) | 1,091 | 5/40 | 488 ms | 983 ms | 1,045 ms | 469 ms |

LIFO High is modelled by sending the prefetches at Interactive, which is LIFO; during a steady scroll no other request is Interactive. The model reproduces the request pattern of the gallery, not WebKit, the IPC bridge or the publisher.

## Options considered

1. **LIFO High queue** (`push_front` for High): a one-line backend change, no frontend change. Rows are then rendered as they enter the overscan, so under mild overload more tiles are ready when they scroll into view (32 of 40 with 8 workers) and the mean wait is lowest. But the visible tiles are still labelled as prefetches. After a stop, every unrendered overscan row ahead (up to `4 × columns` renders, the newest entries) is in front of the visible tiles, the row farthest from the viewport first. That sets the worst case (223 ms against 159 ms with 8 workers), and with few workers it dominates: with 2 workers the median wait is 1.3 s against 0.5 s. Slow renders such as video, capped at two browsing jobs, behave like the 2-worker case.
2. **Drop the sticky `p=1` when a tile whose picture has not arrived becomes visible** (chosen). The new request joins the queued job, and the existing promotion moves it to the front of Interactive. No render is duplicated; the cost is a second request and response (one row read and one file read) for such tiles. High then holds only tiles that have not been visible, and its FIFO order renders the overscan rows ahead of a steady scroll nearest first.
3. **Both.** Under overload Interactive is never empty during a steady scroll, so High order does not matter; when High does run, LIFO would take the row farthest ahead. No gain over option 2.
4. **A visibility or cancellation channel** (a command that demotes or drops jobs of tiles that left the view). It needs a new IPC surface beside the scheme, which ADR 0006 rejected (`thumb_demote`), and WebKitGTK still answers every request.

## Decision

Option 2.

- `GalleryTile` (`src/features/gallery/GalleryTile.tsx`) still takes the prefetch flag when it first shows an asset. It drops it once, for that asset in that tile, when the tile is inside the visible rows (`prefetch` prop false), no fast-scroll pause is active, any cached-only phase is over (a miss after the pause), and the picture has neither shown nor finally failed. The flag never comes back for that asset in that tile, and a picture that has loaded never changes its URL. The retry URL follows the same flag.
- `ThumbnailImage` reports a shown picture through a new `onReady` callback, both on a load event and for a cached picture that is complete without one, and never for the failure placeholder. The tile uses it, together with `onFailed` of the rendering URL, to know that its picture has settled.
- The scheduler is unchanged: High stays FIFO, and a request without `p=1` for a queued High job moves it to the front of Interactive. The `thumb://` URL keeps the same parameters, so the IPC contract does not change.

## Consequences

- The tiles visible where a steady scroll stops render first, before the scrolled-past backlog and before the overscan rows ahead. In the model their median wait drops from 2.3 s to 28 ms (at most 159 ms) with 8 workers, and from 32 s to 0.49 s (at most 1.04 s) with 2 workers.
- A tile that drops `p=1` costs one more `thumb://` request for the same job. Promoted tiles stay at Interactive after they scroll past, so under overload they also keep the overscan rows ahead from rendering during the scroll; rows then arrive unrendered and are drawn after they come into view. This is why fewer tiles are ready at the stop than with LIFO High (16 against 32 of 40 with 8 workers).
- Tests: `GalleryGrid.test.tsx` (the drop, a loaded picture keeping `p=1`, the one-way rule, no drop during a pause) and `ThumbnailImage.test.tsx` (`onReady`) fail without the change. The Rust tests document the backend half that already existed: the steady-scroll order in `thumb_scheduler.rs` and, in `thumb_service.rs`, one render serving both the prefetch and the visible request.
- `steady_scroll_visible_thumbnail_wait_baseline` in `src-tauri/tests/grid_baseline.rs` repeats the measurement. It models the requests; the desktop WebView was not measured.
