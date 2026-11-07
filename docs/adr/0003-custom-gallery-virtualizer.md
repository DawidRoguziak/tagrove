# 0003: Own row-pool virtualizer with a windowed native scroll, without TanStack virtual-core

- Status: Accepted
- Date: 2026-11-07
- Areas: gallery, frontend dependencies, bulk group order, duplicate resolver
- Related: [library query and gallery](../subsystems/library-query-and-gallery.md#virtual-range-loading-and-thumbnail-prefetch), [setup and build](../development/setup-and-build.md), [testing](../development/testing.md#scroll-height-probe), [plan stage 05](../plans/grid-performance/05-custom-virtual-grid.md)

## Context

The gallery used `@tanstack/virtual-core` only as a row-range calculator for fixed-height rows. The adapter in `src/features/gallery/createVirtualGrid.ts` called private `_didMount`/`_willUpdate`, kept its own `version` counter and, above 4,000,000 px, compressed the scroll range in `scrollCoordinates.ts`. That cost:

- every scroll event rebuilt one object per cell and rewrote every tile transform;
- a fast scroll unmounted all tiles, including loaded ones, and remounted them 150 ms later;
- in the compressed mode, a nonpassive wheel listener disabled WebKit's smooth scrolling and each scroll wrote and read `scrollTop`.

Square tiles make every position arithmetic, so a library adds nothing there. `GroupOrderModal` (fixed rows) and `DuplicateResolverDialog` (measured rows) also used virtual-core through the same private-hook pattern.

The stage 05 gate (`e2e/gallery-scale/height-probe.tsx`, WebKitGTK 2.54.2 on private Xvfb, device scales 1 and 2) found that native scrolling, layout, transforms, hit testing and painting stay exact only below 2^24 CSS px (16,777,216). From 16,800,000 px odd offsets round and translated elements stop painting; content saturates at 33,554,428 px. A 1,000,000-item result at six columns is ~33M px tall.

## Options considered

1. **Keep virtual-core and fix the adapter.** Keeps a dependency used as a calculator plus private lifecycle hooks; the compression and the per-event rebuild remain its integration costs.
2. **Compression mapping with a higher cap (previous model).** One physical pixel spans many logical ones: wheel steps need interception, every frame moves all content by a shift, and positions above 2^24 px are inexact anyway.
3. **`overflow: hidden` scroller with a fully custom scroll model (original plan).** JavaScript would own wheel, touchpad inertia and keys. `.gallery-scroll` is shared with the error banner, the footer and the bulk inspector (sticky, or stacked below the gallery under 1000 px), so all of them would have to move into the virtual content. Touchpad inertia would need reimplementation.
4. **Paged virtual scrolling (SlickGrid style).** Native 1:1 scrolling within pages, but the native thumb jumps back at every page boundary.
5. **Own row pool plus a native scroll window with a movable base (chosen).**

## Decision

- `src/features/gallery/gridGeometry.ts` holds pure O(1) geometry: layout, row ranges with four overscan rows, pool size, resize anchors, and the window base. `MAX_GRID_HEIGHT = 16_000_000`.
- `createVirtualGrid.ts` publishes `{ layout, rows }` together and only when rows change. The scroll path reads only `scrollTop`; geometry comes from resize observers. Up to `MAX_GRID_HEIGHT` the spacer is the content and `base` is 0.
- Taller content scrolls natively inside a physical window starting at logical `base`:
  - wheel, touchpad inertia and PageUp/PageDown stay native and 1:1;
  - the base moves (one `scrollTop` write in the same frame as the row positions) after 150 ms idle outside the middle half of the window, or at once within two viewports of an edge;
  - Home/End are handled for the gallery, its scroller and the body;
  - the scroller gets `data-gallery-windowed`: CSS removes its native bar and the gutter comes back as padding, so columns do not change;
  - `GalleryScrollbar.tsx` (`role="scrollbar"`) shows and sets the global position;
  - `scrollShift` = `base` keeps rectangle selection unchanged.
- `GalleryGrid.tsx` renders `ceil(viewport / stride) + 1 + 8` row slots. Slot `s` shows the row congruent to `s` modulo the pool. Each row is one 2D `translate`, `contain: strict` box with a CSS grid and its own group backplates. Slots are ordered by row so DOM order matches visual order. The fast-scroll pause suspends page and thumbnail demand only; loaded tiles stay mounted.
- `ThumbnailImage` keeps one `<img>` (source replacement, hidden until the current source loads, `decoding="async"`).
- `src/ui/dom/createVirtualList.ts` (prefix sums, optional measurement with scroll compensation) replaces virtual-core in both dialogs.
- `@tanstack/virtual-core` is removed from `package.json` and `bun.lock`.

## Consequences

- The gallery creates no tile components while scrolling with cached data. Crossing a row moves two offscreen row nodes. A focused tile loses focus only when its row is recycled, as it did when tiles unmounted.
- Physical positions stay below 2^24 px. `scrollTop = 0` is only the start of the window, so drivers and specs use End/Home, the gallery scrollbar or `[data-testid="gallery-grid-content"]` geometry (updated E2E specs and `verify-mediatagger` recipe).
- The probe fixes the cap only for WebKitGTK 2.54. A WebKitGTK upgrade should rerun `GALLERY_HEIGHT_PROBE=1 sh e2e/gallery-scale/run.sh`.
- Tests:
  - `gridGeometry.test.ts`, `createVirtualGrid.test.tsx`, `GalleryGrid.test.tsx`, `GalleryScrollbar.test.tsx`, `createVirtualList.test.ts`, `GroupOrderModal.test.tsx`;
  - the native fixture `e2e/gallery-scale/fixture.tsx`, which covers 1M windowed scrolling with real input.
- The `GALLERY_BASELINE_DIR` substitution in the scale fixture is gone because old sources need the removed package. Before/after comparisons run the fixture on two commits.
- Stage 08 (timeline scrubber) builds on `scrollToIndex`, `offset`/`maxOffset` and the windowed scrollbar.
