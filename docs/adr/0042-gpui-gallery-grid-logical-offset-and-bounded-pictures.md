# 0042: The GPUI gallery grid scrolls one logical `f64` offset, draws only the rows in range, and bounds its decoded pictures in bytes

- Status: Accepted
- Date: 2026-11-21
- Areas: GPUI port, gallery, thumbnails
- Related: [library query and gallery](../subsystems/library-query-and-gallery.md#gpui-port-the-gallery-grid), [thumbnails](../subsystems/thumbnails.md#gpui-port-tiles-decoding-and-the-picture-cache), [GPUI port plan](../plans/gpui-port/README.md) (epic E09, decision D8), [ADR 0003](0003-custom-gallery-virtualizer.md), [ADR 0021](0021-visible-tiles-drop-a-pending-prefetch.md), [ADR 0024](0024-thumbnail-retries-continue-while-the-tile-is-mounted.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md)

## Context

The gallery must stay smooth for up to 1,000,000 results. The Solid grid ([ADR 0003](0003-custom-gallery-virtualizer.md)) scrolls a native container and works around WebKitGTK's 2^24 px precision limit with a physical window of at most 16,000,000 px whose logical base moves (`MAX_GRID_HEIGHT`, `scrollShift`). GPUI has neither that limit nor that container, but it has two limits of its own:

- **`Pixels` is `f32`.** A million results in one column of the largest tiles are 600,000,000 px. `f32` holds integers exactly only up to 2^24, so a GPUI scroll offset at that depth moves in steps of 32 px and rows jitter. GPUI's own `uniform_list` and `overflow_scroll` keep their offset in `Pixels`.
- **Image textures are never freed.** A decoded picture drawn with `img` is copied into the window's sprite atlas and stays there until `drop_image` is called. The tile rules of [ADR 0021](0021-visible-tiles-drop-a-pending-prefetch.md) and [ADR 0024](0024-thumbnail-retries-continue-while-the-tile-is-mounted.md) say what a tile asks for; in a webview the browser decodes off the main thread and owns the memory. Here both are ours: a decoded thumbnail is 390 × 390 × 4 bytes, about 0.6 MiB, held once as pixels and once as a texture.

## Options considered

1. **`uniform_list` over rows.** Built in and virtualized, but its offset is `f32` and it owns scrolling, so the fast-scroll pause, the resize anchor and a later position bar would have to be reconstructed around it.
2. **A physical window with a moving base, as in the Solid grid.** Keeps `f32` offsets small, at the price of the rebase logic and its edge cases, which exist only because a browser scrolls natively.
3. **One logical `f64` offset in a GUI-free model, and absolutely placed rows (chosen).** The model holds the offset; the view converts only `row top - offset` of the rows in range, which are numbers of the size of the viewport.
4. **GPUI's asset cache for pictures (`img` from bytes).** Decodes off the UI thread, but caches without a bound and gives no hook to release textures.
5. **Our own decode step and a byte-bounded cache that hands dropped pictures back (chosen).**

## Decision

- `media_tagger_client::gallery` holds the grid without a GUI: `geometry` (the functions of `gridGeometry.ts`, in `f64`), `virtual_grid::VirtualGrid` (the scroll state of `createVirtualGrid.ts` as a model), `thumbnail_demand::ThumbnailDemand` (what each tile asks for, `GalleryTile.tsx`) and `picture_cache::PictureCache`.
- `VirtualGrid` keeps one logical offset relative to the grid top in `f64`. It has no physical window, no base and no rebase. Scroll inputs carry their time; the fast-scroll pause ends by a timer effect (150 ms). The thresholds are those of the Solid grid: one step of half a viewport, or a whole viewport within 100 ms.
- The view (`media_tagger_gpui::views::gallery`) is not a GPUI scroll container. It handles the wheel and the scroll keys itself, measures its viewport after layout, and draws the rows in range (the rows in view plus four of overscan on each side) as absolutely placed tiles. Rendering only reads; every change goes through one `refresh`.
- Decoding runs on GPUI's background executor (`media_tagger_gpui::thumbs::Thumbnails`). Decoded pictures live in a `PictureCache` with a byte budget of 256 MiB: pictures of tiles in range are pinned, everything else is dropped least recently used first, so the cache holds at most `max(budget, pinned pictures)`. Every dropped picture is also taken out of the sprite atlas (`App::drop_image`) when the gallery next draws.
- A tile is the thumbnail, the delayed loader and the chip of a video or a GIF, as the Solid tile. The epic's scope also named a favourite marker and an opacity entrance of the picture; the Solid tile has neither (`GalleryTile.tsx` passes no `fadeIn` and draws no favourite mark), so the GPUI tile has neither. The loader keeps its delayed opacity entrance (`.thumbnail-loader`).

## Consequences

- The offset is exact at any depth; `the_offset_is_exact_at_the_millionth_slot` checks a quarter-pixel step at 600,000,000 px. The Solid window tests (`windowHeight`, `maxBase`, `needsRebase`, the windowed scrolling cases) have no counterpart and are not ported.
- Scrolling by wheel is not smoothed and has no momentum of its own: the grid moves by what the platform reports. Scroll keys (Home, End, Page Up, Page Down, Up, Down) are bound in the `Gallery` key context, so they act once the focus is inside the gallery view; with the focus on the window root they do nothing yet (epic E19).
- Geometry is one frame late after a resize: the viewport is measured in the frame's prepaint and applied after it, because nothing may change state while a frame is drawn. The first frame of a gallery view draws no tile.
- Memory is bounded but not small: up to 256 MiB of pixels and the same again as textures when the whole budget has been drawn. A screen whose tiles in range alone exceed the budget (roughly more than 440 tiles) keeps them all; that takes a display beyond 4K at the smallest tile size.
- The position bar and the timeline scrubber (epic E10) drive the same model: `GridInput::ScrollTo`, `JumpToIndex`, `HoldDemand` and `ResumeDemand` exist for them. Selection (epic E11) and the lightbox (epic E16) attach to the tile: `GalleryEvent::OpenAsset` is what activation reports today.
- New invariants and their tests: the cache bound (`gallery/picture_cache.rs`, `tests/gallery_grid.rs`), the range-to-pages mapping (`gallery/virtual_grid.rs`), the tile rules (`gallery/thumbnail_demand.rs`). The files are in `HOT_PATHS` of the review scope collector.
