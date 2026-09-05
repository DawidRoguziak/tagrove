# Library query and gallery

Implementation entry points: [query manager](../../src-tauri/src/services/asset_query_service.rs), [page cache](../../src/hooks/useLibraryAssets.ts), [range loading](../../src/hooks/useLibraryBrowser.ts), [virtual grid](../../src/components/gallery/GalleryGrid.tsx).

This page owns query-session caching, frontend page loading/invalidation, and virtual gallery behavior. It explains how applied search state becomes a revision-bound query and a bounded sparse page cache.

The transport shapes and command-registration rules are in the [IPC contract](../architecture/ipc-contract.md). SQL filters, grouped ordering, and the library revision are described in [database](database.md). Related user workflows belong in [search, tags, and media groups](search-tags-and-media-groups.md), [thumbnails](thumbnails.md), and [lightbox](lightbox.md).

## From applied filters to a session

The top bar has editable filter state and separate applied filter state. A valid submit copies the input, media kind, and favorites flag into the applied state. `useAppShellController` refreshes the asset query when the applied include tags, exclude tags, meta-filter key, media kind, or favorites flag changes. Submitting values that are already applied explicitly refreshes instead. Invalid search metatags do not change the applied state or start a query.

Production uses a page size of 128. `useLibraryAssets.refresh`:

1. increments its local generation;
2. clears the active session reference and in-flight offset set;
3. resets the thumbnail queue and enters loading state;
4. calls `startAssetQuery` with the applied filters, generation, and page size;
5. accepts only a `ready` response for the still-current local generation; and
6. replaces the frontend page cache with the returned page at offset zero.

The local generation rejects late frontend responses. The backend also registers each start before scheduling blocking work and returns `superseded` when a newer start owns the process-wide request. Input normalization belongs to [IPC validation](../architecture/ipc-contract.md#boundary-validation-and-normalization-summary), SQL membership to [database filters](database.md#filter-semantics), and user syntax to [search grammar](search-tags-and-media-groups.md#search-grammar-and-filter-semantics).

## Backend session and page contract

`AssetQueryManager` registers each start in arrival order before blocking work is scheduled; the registration token plus the client generation define the process-wide latest request. It reads the current library revision, builds the ordered ID list, and materializes the first page inside one deferred read transaction, so a ready response reflects a single SQLite snapshot. On a cache miss it asks SQLite for the complete ordered list of matching asset IDs, checking a cooperative cancellation flag periodically so a superseded request aborts mid-build instead of finishing wasted work. That frozen ID vector, rather than full rows, is the session snapshot. The ordering keeps matching media-group members adjacent, orders group buckets by their newest matching member, and then applies group order, modification time, and ID tie-breakers.

The process-wide cache has these bounds:

- at most four sessions;
- expiry after five minutes without cache access;
- most-recently-accessed promotion for lookup by key or session ID; and
- reuse of an equal normalized-filter/revision key, including reuse of its session ID.

The [IPC query states](../architecture/ipc-contract.md#query-session-states) define `ready`, `superseded`, and `stale`. The frontend stores or merges a current `ready` page, ignores a `superseded` start while retaining the old cache, and starts a new query for a `stale` page.

The initial page size and later page limit are clamped by Rust to 1–256. Page offsets use unsigned transport values; an offset past the snapshot end is clamped to `total`. Summary lookup runs in chunks of 500 IDs and reconstructs the snapshot order after SQLite returns rows. The registered legacy `list_assets` command is not the production gallery path; it returns full `Asset` rows, clamps its limit to 1–500, and does not provide a stable session.

Query start, page reads, and detail reads run as blocking work outside the async runtime. Command failures reject with a string as described in the [IPC contract](../architecture/ipc-contract.md).

## Frontend sparse page cache

The cache in `useLibraryAssets` has three coordinated structures:

- `pages: Map<pageOffset, assetId[]>` records which IDs occupy each loaded page;
- `assetsById: Map<assetId, AssetSummary>` stores the summaries verbatim; and
- `lru: pageOffset[]` records the eviction order from the last merge; a bounded access-order ref records subsequent explicit reads.

A merge replaces or adds the page, upserts its assets, and evicts the least recently accessed unprotected page until no more than 12 pages remain. Range access and async page/asset reads promote pages through a bounded ref without setting React state. Render-time `getAssetAt` stays read-only. The latest gallery range protects up to 12 intersecting pages; if an oversized range needs more, the exact cache bound still wins. Eviction deletes the page and its asset objects and the thumbnail-map cleanup effect removes thumbnails for IDs no longer present.

`total` is the session's global match count and is the virtualizer count. By contrast, the returned `assets` array is only `Array.from(assetsById.values())`: it contains cached assets, is not padded to `total`, and must not be indexed as the global result list. `offset` is the greatest merged page end observed and is retained mainly for the reach-end loader.

Global-index access goes through these methods:

- `getAssetAt(index)` computes the aligned page offset, finds the ID at the local page position, and returns the cached asset or `undefined` for a hole.
- `getAssetAtAsync(index)` returns a cached asset or loads the aligned page, then returns that local position. Concurrent lookups for the same missing page await the same in-flight Promise and receive the same page result. Page-load failures reject; an unavailable position can resolve as `undefined`.
- `getAssetIndex(assetId)` scans cached page ID arrays and returns the snapshot index only if that asset's page is loaded; otherwise it returns `null`.
- `getIdsRangeAsync(from, to)` loads every intersecting page and returns the inclusive global ID range. It rejects the whole request when a page is stale, cancelled, failed, or lacks any expected position; callers never receive a silently truncated range.

Only one request per page offset is started at a time. A ref-backed Promise map deduplicates callers, while a separate in-flight counter keeps `loading` true until all tracked query/page requests finish. Refresh clears the Promise map; an older request's `finally` removes its entry only if that exact Promise is still registered, so it cannot erase a newer-generation request for the same offset. A page response is discarded when its captured local generation is obsolete. The active session ID is captured before the request; no page is requested without a session or outside the current `total`. The frontend trusts a backend `ready` page's returned session and revision rather than comparing them again, because the backend page command is keyed by the supplied session ID.

## Summary rows and the details boundary

Sessions materialize `AssetSummary`, which carries display/order metadata and thumbnail/group fields but no byte size or tags. `preview_path` is populated for GIF/video summaries. Gallery tiles do not load original GIFs; the lightbox waits for complete details before presenting any media.

The page cache stores `AssetSummary` objects verbatim: nothing fabricates a path, byte size, or tag list, so a cache entry can never masquerade as a complete record. Gallery tiles read only summary fields (`file_name`, `preview_path`, thumbnail state, kind, media-group fields).

Selecting a tile opens the lightbox with a `SelectedAsset` view built from the summary: `path` starts as the summary's `preview_path` (a valid source for GIF/video; null for ordinary images), and `size_bytes` starts as null. The lightbox media stage renders a loading placeholder until details arrive, or a failure notice when the detail read fails, so no operation consumes an invented path. `useSelectionState` then calls `getAssetDetails`; a successful `AssetDetails` replaces the view with the full path, size, and tags. Selection-request IDs prevent a late detail response from replacing a newer selection. Details for the previous and next global indices are also prefetched, using `getAssetAtAsync` when their pages are absent. A missing detail row or failed detail request leaves the summary view usable. See [lightbox](lightbox.md) for the editing and navigation behavior built on this boundary.

## Virtual range loading and thumbnail prefetch

`useGalleryVirtualGrid` gives TanStack Virtual the row count, `ceil(assetCount / columnCount)`, with one lane and a module-stable row-index key function. Each mounted row expands to its global asset indices. Layout keys are independent of loaded pages; React tile keys remain asset IDs and unloaded slots use `pending-{index}`. Scroll events, thumbnail updates, and page arrivals reuse existing row measurements instead of rebuilding a layout for every asset.

The grid measures its content width and its origin inside the actual scroll container. Tiles remain square with a 10-pixel gap and four rows of overscan on either side. The full-width search/tools header sits outside the gallery scroll container, which fills the remaining window height and owns the scrollbar below both header rows. Header wrapping adjusts that viewport through CSS layout. Bulk mode adds a 360px inspector at widths of at least 1000px; it sticks to the content viewport's top and scrolls independently within that viewport's height. The existing resize observers measure the narrower grid and retain the visible anchor. Changes to tile size or column count invalidate row measurements and keep the previous first visible asset in view. The scroll container, gallery, and grid have resize observation; listeners are removed on unmount.

The range callback reports `{ startIndex, endIndex, visibleStartIndex, visibleEndIndex }`, with inclusive indices. `useLibraryBrowser` loads pages intersecting the overscan range and replaces gallery thumbnail demand with visible and prefetch IDs from currently loaded summaries. Page arrivals and page failure epochs rerun the callback even when the index range stays the same. Empty ranges clear gallery thumbnail demand. Bulk and duplicate preview requests remain additive and independent of this replaceable demand.

Near-end loading retains its two-row threshold for other consumers. Production uses `hasMore: false` and sparse loading across the known total. Thumbnail progress renders outside the memoized grid content, so progress-counter changes alone do not invalidate its layout.

## Gallery presentation states

- If `assetCount` is zero, the grid shows an empty state. It distinguishes “no folders” from “no results” using the scan-root list and offers the add-folder action only for the former.
- If a virtual slot's page is not cached, the grid renders a square pulsing placeholder. Once the summary arrives, the slot becomes an interactive tile.
- Images and videos use a thumbnail when available; otherwise they use a transparent one-pixel placeholder. A preview load error also falls back to that placeholder. The per-tile spinner appears only while the thumbnail store reports that the asset is rendering and no thumbnail path is ready.
- Mounted virtual tiles load their previews eagerly. TanStack Virtual already bounds the mounted range, and native image lazy loading is avoided because WebKitGTK can fail to schedule absolutely positioned, translated virtual items.
- GIFs always use a static JPEG thumbnail or the placeholder, including while idle, scrolling, and after a preview error. Gallery tiles never request original GIF sources. Opening a GIF in the lightbox retains animation. Corner chips identify videos in deep purple and GIFs in deep blue, with colors adapted to the light and dark themes. Photos have no media-kind chip; tile borders use the shared theme border color.
- A nonblank media-group key marks the tile as grouped. Backplates join consecutive loaded virtual entries only when they share the exact key, are adjacent lanes, and sit on the same row; a group that crosses a row or unloaded gap is drawn as separate pieces.
- The status footer reports thumbnail generation only when `hasMore` is true, and reports “no more items” when thumbnail generation is idle and `hasMore` is false. It does not represent asset-page loading. Production therefore normally treats the known session total as the end boundary.

## Refresh and invalidation paths

The database revision is the authoritative invalidation epoch. Scans and query-visible mutations bump it according to [database](database.md), so a later page from an older session becomes `stale`. The frontend also refreshes eagerly on these paths:

| Trigger | Frontend behavior |
| --- | --- |
| Applied filters change | The controller effect calls asset `refresh`; resubmitting identical applied filters calls it directly. A ready first page replaces the old cache. |
| Scan/rescan, scan-root removal, CSV import, duplicate rename/delete | The settings workflow calls `refreshLibrary`, which refreshes assets and known tags together. Root removal first resets the thumbnail queue. |
| Database restore | The backend closes the maintenance gate, clears the process-wide query manager, drains/invalidates the old database pool, and clears again after successful installation. The frontend invalidates tag/details identity state inside its mutation barrier, resets the thumbnail queue and map, refreshes roots, then refreshes assets and known tags. |
| Clear library | After backend success, the frontend immediately resets the thumbnail queue, thumbnails, cached assets, total, offset, and known tags. It does not need to query the now-empty library. |
| Lightbox delete | Removes the loaded object and selection locally, refreshes known tags, then starts a fresh asset query. |
| Bulk favorite toggle | Uses all selected IDs, patches loaded summaries and both detail caches, and refreshes after either toggle direction in a favorites-only query or after missing IDs. Existing selected IDs survive the refresh. |
| Favorite toggle | Patches loaded state. Removing an item while favorites-only is applied also refreshes; other favorite changes do not. |
| Single/bulk tag edit | Patches loaded objects locally. Tag operations also refresh known tags. When the changed tags intersect an applied include/exclude filter, the query restarts immediately so membership is re-evaluated. Otherwise a later uncached page observes the bumped revision as `stale` and refreshes. |
| Single/bulk media-group edit | Patches loaded objects locally and then always restarts the asset query, because group changes alter ordering and adjacency of the active view. |
| Thumbnail generation or cleanup | Updates thumbnail state without changing query membership/order and does not bump the library revision. The thumbnail store is authoritative for production tiles. |

Starting a refresh resets the active thumbnail queue but keeps the old asset cache and `total` visible until a current `ready` first page replaces them. Replacement clears both page IDs and `assetsById`, so rows retained only by an older session cannot leak into the new compact cache. Old start and page responses cannot merge after the local generation changes.

## Error behavior

An invoke or database error rejects the corresponding promise and records a gallery-level error state. `useLibraryAssets` exposes `loadError`, `retryLoad`, and `pageFailureEpoch`: a failed start sets `loadError`, keeps the previous cache visible, and `retryLoad` clears the error and starts a fresh query; a failed page load also sets `loadError`, leaves the failed offset as a retryable hole, and bumps `pageFailureEpoch` so the virtual range dedup marker re-opens and the range callback re-requests the missing page. The gallery grid renders an alert banner with a retry button while `loadError` is set. A `superseded` start and `stale` page are normal result variants rather than errors; only `stale` causes an automatic refresh. Detail failures have separate lightbox loading/error/Retry state and do not become gallery page errors. Calls made by settings workflows continue to surface through their operation runner.

## Known limitations

- Backend supersession, TTL, and the four-session LRU are process-wide. Another window or independent caller can supersede an uncached start or evict this window's session.
- `assets` and bulk-selection operations cover loaded pages only, and `getAssetIndex` cannot locate an unloaded asset. The 12-page bound takes precedence if an unusually large requested viewport intersects more than 12 pages.
- The detail caches in `useSelectionState` and `useBulkSelectionController` are bounded LRU maps (256 entries) stamped with the tag-state identity epoch and cleared whenever a new query session starts. Reselecting an asset after a restore, identity reset, or session restart re-reads details instead of trusting stale path/size/tags. Ordinary favorite/group mutations still rely on the authoritative-tag overlay and local patches rather than a per-revision invalidation.
- Refresh does not cancel in-flight backend page work; the generation guard only discards late responses after they finish. A `superseded` result is not retried automatically.
- Tag-edit invalidation checks applied include/exclude tags but does not check the `tags:N` exact-count meta-filter. A count-filter view can therefore retain stale membership until explicit refresh or stale-page recovery.
- The empty state depends only on `assetCount`, so it can appear during the initial load while `total` is still zero. Conversely, an in-progress refresh keeps the old nonempty grid visible. The footer does not expose asset-page loading and normally shows the end state whenever thumbnail generation is idle.
- Sparse slots change React keys from `pending-{index}` to the asset ID when loaded, which remounts that tile. Group backplates are presentation-only fragments for loaded entries in one row; they do not visually span row boundaries or holes.
- The connection pool bounds each acquisition at five seconds but aggregate retry loops can still wait longer; the pool's busy path has no focused unit test yet.

## Safe change checklist

1. Keep the Rust response enums, serialized snake-case fields, TypeScript unions, wrappers, and registered commands synchronized. Update the [IPC contract](../architecture/ipc-contract.md) for any transport or state change.
2. Preserve one canonical filter normalization and the complete SQL ordering tuple. Check tag AND/NOT, favorite, kind, exact tag-count, exact normalized group-name, group bucket, null group order, modification time, and ID tie-break behavior in [database](database.md).
3. Decide whether every write changes query membership or order. Bump the library revision on the backend and either refresh immediately or deliberately document a local patch that preserves all relevant membership and ordering invariants.
4. If page size or cache limits change, validate a positive frontend page size, keep backend clamps explicit, test boundary offsets, and prove the frontend never retains more than the intended exact page limit.
5. Treat `total` and cached `assets` as different quantities. Use global-index accessors for virtualized navigation and selection; do not index the loaded-assets array as though it were the session snapshot.
6. Keep `AssetSummary` lean. If a gallery feature needs full path, size, or tags, either justify adding that cost to every summary or cross the `AssetDetails` boundary and define cache invalidation for the added data.
7. Preserve generation checks, per-offset request de-duplication, loading-counter balance, stale recovery, and thumbnail-queue reset. Add explicit cancellation or error state before relying on either behavior in the UI.
8. Test virtual holes, cache eviction and reload, resize/overscan ranges, placeholder-to-tile transitions, thumbnail prefetch after a page merge, zero-result loading, group backplates, and static GIF previews with missing, ready, and failed thumbnails.
9. Add backend tests for equal-key reuse, supersession, revision mismatch, missing/expired/evicted sessions, LRU promotion, TTL, page clamps, and cross-session isolation. Use a controllable clock or cache constructor rather than sleeping for the TTL.
10. Run Rust formatting and the relevant backend query/database tests, then run the focused React hook, gallery, selection, search-filter, lightbox, and thumbnail tests. For cache or virtualizer changes, also exercise a library larger than 12 pages and scroll backward into evicted ranges.

### Relevant existing tests

- `src-tauri/src/db.rs` tests cover filter composition, grouped ordering, summary-supporting data invariants, cooperative ID-build cancellation, and related mutations including same-transaction revision bumps.
- `src-tauri/src/services/asset_query_service.rs` has a focused unit-test module covering equal-key session reuse, registration-order and generation supersession, revision-stale pages, LRU/TTL eviction, clear semantics, and snapshot starts.
- `src/hooks/__tests__/useLibraryAssets.test.tsx` covers first-page replacement, superseded-start retention, start/page failure error state with retry recovery, stale-triggered restarts, page-failure epoch signaling, and late-generation response rejection.
- `src-tauri/tests/backend_integration.rs` covers file-backed asset/tag/query flows, although it does not exercise `AssetQueryManager` session policy directly.
- `src/hooks/__tests__/useLibraryBrowser.test.ts` covers first-page replacement, filter forwarding, duplicate reach-end suppression, shared in-flight indexed page lookup, virtual-range thumbnail IDs, and clear-library reset.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx` covers virtual-range reporting, reach-end triggering, thumbnail status/spinners, selection interactions, static GIF previews, and isolation of thumbnail/progress updates from grid rendering.
- `src/hooks/__tests__/useSelectionState.test.ts` covers detail/mutation races, identity-reset ID reuse, deletion tombstones, query-session cache invalidation, rapid navigation, and query-position recomputation. See [lightbox tests](lightbox.md#tests) for remaining navigation/prefetch scenarios.
- `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts` covers applied-state transitions, meta-filter validation, and explicit refresh when filters are unchanged.

### Gallery performance verification

`src/components/gallery/hooks/__tests__/useGalleryVirtualGrid.test.tsx` runs the installed TanStack virtualizer with 10,000 numeric slots and asserts that scrolling and page/status updates do not rebuild row measurements. It also checks external-scroll offsets, partial rows, and resize anchoring. Queue/store tests cover latest-viewport priority, additive consumers, reset/unmount races, atomic tile updates, and version cleanup; page-cache tests cross the 12-page bound and reload evicted pages.

The opt-in desktop spec uses 2,048 copied PNGs, 12 GIFs, and two videos in a temporary root. It follows the same route with cold and warm thumbnails, visits more than 12 pages, returns to evicted ranges, bounds mounted tiles, and compares decoded pixels from native-display captures of lightbox GIF frames. Run with `MEDIATAGGER_GALLERY_PERF=1 bun run test:e2e:tauri --spec e2e/specs/gallery.performance.e2e.js` in the isolated desktop environment described in [testing](../development/testing.md). For headless Linux, use `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 MEDIATAGGER_GALLERY_PERF=1 xvfb-run -a -s '-screen 0 1920x1080x24' bun run test:e2e:tauri --spec e2e/specs/gallery.performance.e2e.js`. The pixel check requires ImageMagick `import` and `magick`. It writes samples to `artifacts/gallery-performance/`. Timings include WebDriver overhead and are observations, not portable frame-rate thresholds. Million-file desktop behavior requires manual validation; no million-file dataset is generated.
