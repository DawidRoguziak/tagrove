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
- At most 24 pages (`MAX_CACHED_PAGES`) are kept (LRU of unprotected pages), with one request per page offset at a time. Only `ready` responses for the current generation are accepted.
- The page cache is mutated in place; every change bumps the `version` signal. Any render-time read (`getAssetAt`, `assets`) must read `version`, or pooled grid cells stop updating.
- There is no thumbnail queue or store: tiles load `thumb://` URLs from each summary's `thumb_version`. Old viewport pages stay as render-only fallback (`getAssetAt` only), never visible to `getAssetIndex`, async reads or ranges.
- `stale` responses schedule one debounced refresh (250 ms, at most one per second); explicit refreshes run at once. There is no `hasMore`/reach-end loader: viewport demand is the only page loader.
- `getIdsRangeAsync` reads the backend session vector through the binary `get_asset_query_id_range` (f64 LE `[count, ...ids]`, empty body = stale; ADR 0005), never through pages, and rejects the whole range instead of truncating it.
- A refresh passes the first visible index as `anchorOffset`; the `ready` result carries that page and its `offset`.
- The scale target is up to 1,000,000 results. The gallery virtualizer is our own (`gridGeometry.ts`, `createVirtualGrid.ts`; ADR 0003); tests cover 10k/26k/1M slots.
- `GalleryGrid` recycles a fixed pool of row slots: scrolling must not create tile components or read layout beyond `scrollTop`. The fast-scroll pause suspends page demand; assets newly shown during it request cached thumbnails only (`c=1`).
- The physical spacer is capped at `MAX_GRID_HEIGHT` (16,000,000 px, below WebKitGTK's 2^24 px precision limit). Taller results scroll natively inside a window at logical `base` (= `scrollShift`); `scrollTop` 0 is only the window start.
- Invalidation is decided by `library/metadata/queryImpact.ts` (`none` / `tags` / `all`).
