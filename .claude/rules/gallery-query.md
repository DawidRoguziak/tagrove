---
paths:
  - "src/library/**"
  - "src/features/gallery/**"
  - "src/app/AppGalleryView.tsx"
---

# Library query and gallery

Read `docs/subsystems/library-query-and-gallery.md`.

- Backend query sessions return pages of `AssetSummary` (no tags or byte size). The frontend page cache stores summaries verbatim and never fabricates detail fields. Details come from `get_asset_details` through the shared details cache.
- `total` is the virtualizer count. The `assets` array holds only cached rows, so never index it as the global result list.
- At most 12 pages are kept (LRU of unprotected pages), with one request per page offset at a time. Only `ready` responses for the current generation are accepted.
- `getIdsRangeAsync` rejects the whole range instead of truncating it.
- The scale target is up to 1,000,000 results. The gallery virtualizer is our own (`gridGeometry.ts`, `createVirtualGrid.ts`; ADR 0003); tests cover 10k/26k/1M slots.
- `GalleryGrid` recycles a fixed pool of row slots: scrolling must not create tile components or read layout beyond `scrollTop`. The fast-scroll pause suspends only page and thumbnail demand.
- The physical spacer is capped at `MAX_GRID_HEIGHT` (16,000,000 px, below WebKitGTK's 2^24 px precision limit). Taller results scroll natively inside a window at logical `base` (= `scrollShift`); `scrollTop` 0 is only the window start.
- Invalidation is decided by `library/metadata/queryImpact.ts` (`none` / `tags` / `all`).
