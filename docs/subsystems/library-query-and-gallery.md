# Library query and gallery

This subsystem turns applied search state into a revision-bound asset query, keeps a bounded sparse page cache in React, and renders the library as a virtual grid. The Rust commands and models, `src/api.ts` and `src/types.ts`, `useLibraryAssets`, `useLibraryBrowser`, and the gallery components are the source of truth.

The transport shapes and command-registration rules are in the [IPC contract](../architecture/ipc-contract.md). SQL filters, grouped ordering, and the library revision are described in [database](database.md). Related user workflows belong in [search, tags, and media groups](search-tags-and-media-groups.md), [thumbnails](thumbnails.md), and [lightbox](lightbox.md).

## Current guarantees

### From applied filters to a session

The top bar has editable filter state and separate applied filter state. A valid submit copies the input, media kind, and favorites flag into the applied state. `useAppShellController` refreshes the asset query when the applied include tags, exclude tags, meta-filter key, media kind, or favorites flag changes. Submitting values that are already applied explicitly refreshes instead. Invalid search metatags do not change the applied state or start a query.

Production uses a page size of 128. `useLibraryAssets.refresh`:

1. increments its local generation;
2. clears the active session reference and in-flight offset set;
3. resets the thumbnail queue and enters loading state;
4. calls `startAssetQuery` with the applied filters, generation, and page size;
5. accepts only a `ready` response for the still-current local generation; and
6. replaces the frontend page cache with the returned page at offset zero.

The frontend generation is a late-response guard only. Rust accepts the `generation` field for compatibility but does not use it. Command normalization trims, lowercases, removes empty tags, and de-duplicates them; supported kinds are `image`, `gif`, and `video`, while an unsupported kind becomes no kind filter. A negative exact tag-count or a blank group-name meta-filter rejects the command. Full filter semantics are documented with the SQL model in [database](database.md) and the input grammar in [search, tags, and media groups](search-tags-and-media-groups.md).

### Backend session and page contract

`AssetQueryManager` reads the current library revision and forms a cache key from that revision plus all normalized filters. On a cache miss it asks SQLite for the complete ordered list of matching asset IDs. That frozen ID vector, rather than full rows, is the session snapshot. The ordering keeps matching media-group members adjacent, orders group buckets by their newest matching member, and then applies group order, modification time, and ID tie-breakers.

The process-wide cache has these bounds:

- at most four sessions;
- expiry after five minutes without cache access;
- most-recently-accessed promotion for lookup by key or session ID; and
- reuse of an equal normalized-filter/revision key, including reuse of its session ID.

The three successful protocol states are:

| State | Command | Meaning and frontend response |
| --- | --- | --- |
| `ready` | start or page | Includes `session_id`, `revision`, global `total`, effective `offset`, and ordered `AssetSummary[]`. The frontend stores the session on start and merges a page only while its local generation is current. |
| `superseded` | start only | A cache-miss ID build finished after another process-wide start became latest. It is not an IPC error. The frontend ignores it and retains the previous cache; it does not automatically retry. Cache hits return `ready` without this post-build supersession check. |
| `stale` | page only | The session is absent, expired, evicted, or has a revision different from the database. Revision mismatch also removes it. The frontend starts a new query. |

The initial page size and later page limit are clamped by Rust to 1–256. Page offsets use unsigned transport values; an offset past the snapshot end is clamped to `total`. Summary lookup runs in chunks of 500 IDs and reconstructs the snapshot order after SQLite returns rows. The registered legacy `list_assets` command is not the production gallery path; it returns full `Asset` rows, clamps its limit to 1–500, and does not provide a stable session.

Query start, page reads, and detail reads run as blocking work outside the async runtime. Command failures reject with a string as described in the [IPC contract](../architecture/ipc-contract.md).

### Frontend sparse page cache

The cache in `useLibraryAssets` has three coordinated structures:

- `pages: Map<pageOffset, assetId[]>` records which IDs occupy each loaded page;
- `assetsById: Map<assetId, Asset>` stores the summary-derived objects; and
- `lru: pageOffset[]` records page merge recency, newest first.

A merge replaces or adds the page, upserts its assets, moves that page offset to the front, and evicts from the back until exactly no more than 12 pages remain. Eviction deletes the page and its asset objects and the thumbnail-map cleanup effect removes thumbnails for IDs no longer present. A cache hit does not currently promote the page, so the policy is merge-recency rather than true read-recency.

`total` is the session's global match count and is the virtualizer count. By contrast, the returned `assets` array is only `Array.from(assetsById.values())`: it contains cached assets, is not padded to `total`, and must not be indexed as the global result list. `offset` is the greatest merged page end observed and is retained mainly for the reach-end loader.

Global-index access goes through these methods:

- `getAssetAt(index)` computes the aligned page offset, finds the ID at the local page position, and returns the cached asset or `undefined` for a hole.
- `getAssetAtAsync(index)` returns a cached asset or loads the aligned page, then returns that local position. Concurrent lookups for the same missing page await the same in-flight Promise and receive the same page result. Page-load failures reject; an unavailable position can resolve as `undefined`.
- `getAssetIndex(assetId)` scans cached page ID arrays and returns the snapshot index only if that asset's page is loaded; otherwise it returns `null`.

Only one request per page offset is started at a time. A ref-backed Promise map deduplicates callers, while a separate in-flight counter keeps `loading` true until all tracked query/page requests finish. Refresh clears the Promise map; an older request's `finally` removes its entry only if that exact Promise is still registered, so it cannot erase a newer-generation request for the same offset. A page response is discarded when its captured local generation is obsolete. The active session ID is captured before the request; no page is requested without a session or outside the current `total`. The frontend trusts a backend `ready` page's returned session and revision rather than comparing them again, because the backend page command is keyed by the supplied session ID.

### Summary rows and the details boundary

Sessions materialize `AssetSummary`, not full `Asset` rows. A summary contains display and ordering metadata, thumbnail state, and media-group fields, but not byte size or tags. SQLite puts the source path in `preview_path` for GIF and video summaries so either media type can start from a valid source before the separate detail read. Ordinary image summaries keep it null.

For compatibility with gallery and mutation code, `summaryToAsset` creates an intentionally incomplete `Asset`:

- `path` is `preview_path` for a GIF or video and otherwise the file name;
- `size_bytes` is `0`; and
- `tags` is `[]`.

These values are placeholders, not claims about the underlying file. Selecting a tile immediately opens the lightbox with the summary-derived object, then `useSelectionState` calls `getAssetDetails`. A successful `AssetDetails` replaces it with the full path, size, and tags. Selection-request IDs prevent a late detail response from replacing a newer selection. Details for the previous and next global indices are also prefetched, using `getAssetAtAsync` when their pages are absent. A missing detail row or failed detail request leaves the summary usable. See [lightbox](lightbox.md) for the editing and navigation behavior built on this boundary.

### Virtual range loading and thumbnail prefetch

`GalleryGrid` gives TanStack Virtual the global `assetCount`, the sparse `getAssetAt`, and stable asset IDs when loaded; unloaded slots temporarily use `pending-{index}` keys. The grid derives its column count from measured width, uses a 10-pixel gap, and overscans by at least 12 items or four rows' worth of columns.

Whenever the virtual range changes, `useLibraryBrowser` asks `ensureRange` to load every aligned page intersecting the range. It then queues thumbnails only for assets already present in that range. Merging a page changes the indexed accessor, resets the virtual range's deduplication marker, and allows the range callback to run again so the newly loaded IDs can enter the thumbnail queue. Thumbnail scheduling, streaming, and its independent generation guard are documented in [thumbnails](thumbnails.md).

`useGalleryVirtualGrid` also has a near-end callback threshold of two rows. In the production controller `hasMore` is currently always `false`, because sparse range loading spans the known global count; consequently the near-end callback and `handleReachEnd` are not the primary production loading mechanism.

### Gallery presentation states

- If `assetCount` is zero, the grid shows an empty state. It distinguishes “no folders” from “no results” using the scan-root list and offers the add-folder action only for the former.
- If a virtual slot's page is not cached, the grid renders a square pulsing placeholder. Once the summary arrives, the slot becomes an interactive tile.
- Images and videos use a thumbnail when available; otherwise they use a transparent one-pixel placeholder. A preview load error also falls back to that placeholder. The per-tile spinner appears only while the thumbnail store reports that the asset is rendering and no thumbnail path is ready.
- Mounted virtual tiles load their previews eagerly. TanStack Virtual already bounds the mounted range, and native image lazy loading is avoided because WebKitGTK can fail to schedule absolutely positioned, translated virtual items.
- GIFs use their summary `preview_path` for animation only while scrolling is idle, the tile intersects the measured viewport, and at most ten GIFs are visible. Otherwise they use a thumbnail or the placeholder. Video and GIF chips and colored borders identify those kinds.
- A nonblank media-group key marks the tile as grouped. Backplates join consecutive loaded virtual entries only when they share the exact key, are adjacent lanes, and sit on the same row; a group that crosses a row or unloaded gap is drawn as separate pieces.
- The status footer reports thumbnail generation only when `hasMore` is true, and reports “no more items” when thumbnail generation is idle and `hasMore` is false. It does not represent asset-page loading. Production therefore normally treats the known session total as the end boundary.

### Refresh and invalidation paths

The database revision is the authoritative invalidation epoch. Scans and query-visible mutations bump it according to [database](database.md), so a later page from an older session becomes `stale`. The frontend also refreshes eagerly on these paths:

| Trigger | Frontend behavior |
| --- | --- |
| Applied filters change | The controller effect calls asset `refresh`; resubmitting identical applied filters calls it directly. A ready first page replaces the old cache. |
| Scan/rescan, scan-root removal, CSV import, duplicate rename/delete | The settings workflow calls `refreshLibrary`, which refreshes assets and known tags together. Root removal first resets the thumbnail queue. |
| Database restore | The backend clears the process-wide query manager and invalidates the database pool. The frontend resets the thumbnail queue and thumbnail map, refreshes roots, then refreshes assets and known tags. |
| Clear library | After backend success, the frontend immediately resets the thumbnail queue, thumbnails, cached assets, total, offset, and known tags. It does not need to query the now-empty library. |
| Lightbox delete | Removes the loaded object and selection locally, refreshes known tags, then starts a fresh asset query. |
| Favorite toggle | Patches loaded state. Removing an item while favorites-only is applied also refreshes; other favorite changes do not. |
| Single/bulk tag or media-group edit | Patches loaded objects locally. Tag operations also refresh known tags, but these operations do not immediately restart the asset query. A later uncached page observes the bumped revision as `stale` and refreshes. |
| Thumbnail generation or cleanup | Updates thumbnail state without changing query membership/order and does not bump the library revision. The thumbnail store is authoritative for production tiles. |

Starting a refresh resets the active thumbnail queue but keeps the old asset cache and `total` visible until a current `ready` first page replaces them. Old start and page responses cannot merge after the local generation changes.

### Error behavior

An invoke or database error rejects the corresponding promise. `refresh` always balances its internal loading counter but does not clear the old cache, set an error object, or retry. Calls made by settings workflows are surfaced by their operation runner. Calls launched with `void` from virtual range loading have no gallery-local error UI. A `superseded` start and `stale` page are normal result variants rather than errors; only `stale` causes an automatic refresh. Detail failures are deliberately swallowed so the selected summary remains viewable.

## Known limitations

- A session snapshots IDs and order, not rows. Query start does not hold one read transaction across revision read, ID selection, and first-page materialization, and it does not re-check revision before returning. A concurrent mutation can therefore produce a mixed initial page; later page access becomes stale after a successful revision bump.
- Backend supersession, TTL, and the four-session LRU are process-wide. Another window or independent caller can supersede an uncached start or evict this window's session.
- The frontend cache's LRU list is updated only when a page merges, not when it is read. `assets` and bulk-selection operations cover loaded pages only, and `getAssetIndex` cannot locate an unloaded asset.
- Tag and media-group edits can change filter membership, group adjacency, or global order, but their current lightbox and bulk actions only patch loaded objects. Until a refresh or stale page is encountered, the visible order and membership can disagree with the database snapshot. A media-group patch also does not rebuild cached page ID arrays.
- The detail cache in `useSelectionState` has no revision or lifecycle invalidation. Reselecting an asset after a mutation or database restore can reuse old path, size, tags, or other detail fields. The selection synchronization effect deliberately preserves the current full path and size and can preserve current tags when refreshed summaries carry the expected empty placeholder list.
- Refresh does not cancel backend work or immediately clear the old gallery. A `superseded` result is not retried, and an error leaves the previous results visible without a gallery-specific error message.
- The empty state depends only on `assetCount`, so it can appear during the initial load while `total` is still zero. Conversely, an in-progress refresh keeps the old nonempty grid visible. The footer does not expose asset-page loading and normally shows the end state whenever thumbnail generation is idle.
- Sparse slots change React keys from `pending-{index}` to the asset ID when loaded, which remounts that tile. Group backplates are presentation-only fragments for loaded entries in one row; they do not visually span row boundaries or holes.
- `asset_query_service.rs` has no focused Rust unit-test module. The frontend has no focused `useLibraryAssets` suite for stale recovery, superseded starts, generation races, exact 12-page eviction, sparse lookup, or page-load errors. Existing browser tests cover only a subset of these behaviors.

## Safe change checklist

1. Keep the Rust response enums, serialized snake-case fields, TypeScript unions, wrappers, and registered commands synchronized. Update the [IPC contract](../architecture/ipc-contract.md) for any transport or state change.
2. Preserve one canonical filter normalization and the complete SQL ordering tuple. Check tag AND/NOT, favorite, kind, exact tag-count, exact normalized group-name, group bucket, null group order, modification time, and ID tie-break behavior in [database](database.md).
3. Decide whether every write changes query membership or order. Bump the library revision on the backend and either refresh immediately or deliberately document a local patch that preserves all relevant membership and ordering invariants.
4. If page size or cache limits change, validate a positive frontend page size, keep backend clamps explicit, test boundary offsets, and prove the frontend never retains more than the intended exact page limit.
5. Treat `total` and cached `assets` as different quantities. Use global-index accessors for virtualized navigation and selection; do not index the loaded-assets array as though it were the session snapshot.
6. Keep `AssetSummary` lean. If a gallery feature needs full path, size, or tags, either justify adding that cost to every summary or cross the `AssetDetails` boundary and define cache invalidation for the added data.
7. Preserve generation checks, per-offset request de-duplication, loading-counter balance, stale recovery, and thumbnail-queue reset. Add explicit cancellation or error state before relying on either behavior in the UI.
8. Test virtual holes, cache eviction and reload, resize/overscan ranges, placeholder-to-tile transitions, thumbnail prefetch after a page merge, zero-result loading, group backplates, and GIF animation at 10 and 11 visible GIFs.
9. Add backend tests for equal-key reuse, supersession, revision mismatch, missing/expired/evicted sessions, LRU promotion, TTL, page clamps, and cross-session isolation. Use a controllable clock or cache constructor rather than sleeping for the TTL.
10. Run Rust formatting and the relevant backend query/database tests, then run the focused React hook, gallery, selection, search-filter, lightbox, and thumbnail tests. For cache or virtualizer changes, also exercise a library larger than 12 pages and scroll backward into evicted ranges.

### Relevant existing tests

- `src-tauri/src/db.rs` tests cover filter composition, grouped ordering, summary-supporting data invariants, and related mutations.
- `src-tauri/tests/backend_integration.rs` covers file-backed asset/tag/query flows, although it does not exercise `AssetQueryManager` session policy directly.
- `src/hooks/__tests__/useLibraryBrowser.test.ts` covers first-page replacement, filter forwarding, duplicate reach-end suppression, shared in-flight indexed page lookup, virtual-range thumbnail IDs, and clear-library reset.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx` covers virtual-range reporting, reach-end triggering, thumbnail status/spinners, selection interactions, and the 10-GIF animation threshold.
- `src/hooks/__tests__/useSelectionState.test.ts` and the lightbox service tests cover selection/editor synchronization, local mutation patches, conditional favorite refresh, and delete refresh. Detail loading, its request guard, adjacent prefetch, and detail-cache invalidation are not directly covered.
- `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts` covers applied-state transitions, meta-filter validation, and explicit refresh when filters are unchanged.
