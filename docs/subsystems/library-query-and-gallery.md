# Library query and gallery

Implementation entry points: [query manager](../../src-tauri/src/services/asset_query_service.rs), [page cache](../../src/library/query/createAssetQuery.ts), [range loading](../../src/library/createLibrary.ts), [virtual grid](../../src/features/gallery/GalleryGrid.tsx).

This page owns query-session caching, frontend page loading/invalidation, and virtual gallery behavior. It explains how applied search state becomes a revision-bound query and a bounded sparse page cache.

The transport shapes and command-registration rules are in the [IPC contract](../architecture/ipc-contract.md). SQL filters, grouped ordering, and the library revision are described in [database](database.md). Related user workflows belong in [search, tags, and media groups](search-tags-and-media-groups.md), [thumbnails](thumbnails.md), and [lightbox](lightbox.md).

## Frontend ownership

`src/library/createLibrary.ts` composes the shared models under
`createAppRuntime`; feature views do not construct separate library owners.
`query/createAssetQuery.ts` preserves 128-item pages, sparse positions, a 12-page cache,
failed offsets, explicit retry, patch-counter rereads and stale-session recovery.
Operational state changes synchronously, while getters publish through Solid. Separate
memos prevent loading-only changes from republishing the cached asset array. Lightbox commands
use `navigationSnapshot()` for the synchronous epoch, pending state and total, so a key press
in the same batch as refresh cannot use the previous session.

Viewport or query-epoch changes request pages. Page arrivals update thumbnail demand for
the current viewport without requesting evicted pages again. This keeps oversized ranges
from continuously reloading pages beyond the cache bound. Metadata patches preserve page
positions, including holes after deletion, until a new query replaces the snapshot.

`metadata/createDetailsCache.ts` shares at most 256 complete details across Solid
consumers. It deduplicates reads, pins pending identities, validates metadata generations,
and discards replies after query changes, identity resets or disposal. Query changes clear
details without resetting canonical tags. Restore and clear invalidate both. The gallery
consumes these models through `app/AppGalleryView.tsx`.
The application-owned lightbox also consumes the details cache, canonical tags and mutations.
Mutation writes capture the details-cache generation before IPC. A successful save can
restore its captured complete record only if that generation still matches. A query refresh
or explicit clear therefore prevents a delayed save from restoring old path/size fields;
accepted canonical metadata still publishes to tags, summaries and the current selection.

`features/gallery/createVirtualGrid.ts` adapts the pinned `@tanstack/virtual-core` dependency. It waits for measured grid width before
publishing nonempty ranges, owns observers under the mounted view and preserves global-index
resize anchors. Thumbnail completion does not rebuild row measurements. Tiles use stable
global-index keys, and each loaded tile subscribes only to its current asset's thumbnail.
The image element resets its load/failure/fade state when its source changes.

The current tests run real virtual-core at 10,000, 26,000 and 1,000,000 slots, drive sparse replacement,
explicit page retry and eviction/reload across the 12-page bound, and exercise the native
WebView with synthetic collections. See the [stage evidence](../solid2/PROGRESS.md) and
[browsing desktop recipe](../development/testing.md#solid-feature-verification).

## From applied filters to a session

The top bar has editable filter state and separate applied filter state. A valid submit copies the input, media kind, and favorites flag into the applied state. `createSearch.submit` passes the normalized descriptor to `createLibrary.applyFilters`, which refreshes the query. Every valid submission refreshes, including one whose descriptor is unchanged. Invalid search metatags do not change the applied state or start a query.

Production uses a page size of 128. `createAssetQuery.refresh`:

1. increments its local generation and query epoch, marks the query pending, and invalidates navigation positions;
2. clears the active session reference and in-flight offset set;
3. resets the thumbnail queue and enters loading state;
4. calls `startAssetQuery` with the applied filters, owner ID, generation, and page size;
5. accepts only a `ready` response for the still-current local generation; and
6. advances the query epoch again and replaces the frontend page cache with the returned page at offset zero.

Each query model creates a random owner ID for its lifetime. The local generation rejects late frontend responses. Native async admission can run out of call order, so the backend retains the highest admitted generation per owner for the process lifetime. An equal or lower generation gets an obsolete token without replacing the current request. A new owner starts with an independent generation counter. Accepted registrations still share one process-wide request token. Input normalization belongs to [IPC validation](../architecture/ipc-contract.md#boundary-validation-and-normalization-summary), SQL membership to [database filters](database.md#filter-semantics), and user syntax to [search grammar](search-tags-and-media-groups.md#search-grammar-and-filter-semantics).

## Backend session and page contract

`AssetQueryManager` compares owner generations under an admission mutex before scheduling blocking work; one authoritative registration token defines the runtime's latest accepted request. It reads the global and dependency revisions, builds the ordered ID list, and materializes the first page inside one deferred read transaction, so a ready response reflects a single SQLite snapshot. On a cache miss it asks SQLite for the complete ordered list of matching asset IDs, using a SQLite progress callback during execution and sorting as well as row-loop checks so a superseded request stops work. That frozen ID vector, rather than full rows, is the session snapshot. The ordering keeps matching media-group members adjacent, orders group buckets by their newest matching member, and then applies group order, modification time, and ID tie-breakers.

Each session stores its dependency keys and their maximum revision. Only a changed dependency makes its pages or position lookups stale. Unrelated tags and favorites preserve IDs, order, total, and the cached session ID, while the public `revision` advances with the read snapshot. Dependency keys and initialization rules belong to [database revisions](database.md#library-revision-and-query-sessions).

The application-owned cache has these bounds:

- at most four sessions and 64 MiB of retained ID-vector capacity;
- expiry after five minutes without cache access;
- most-recently-accessed promotion for lookup by key or session ID; and
- reuse of an equal normalized-filter/dependency-revision key, including reuse of its session ID.

A separate cache epoch changes on clear. Insertion checks both the request token and epoch under the cache mutex, preventing an in-flight build from repopulating a cleared cache. SQLite progress callbacks are removed before connection reuse. An authoritative session larger than the retained-ID budget rejects with a text error asking the user to narrow the search. It does not return `superseded`, evict existing sessions, or retry automatically. Empty and populated frontend views expose the query error and Retry; a new filter submission can recover with a fitting query. The retained-ID budget excludes transient SQLite allocations and vectors still held by in-flight readers; the [backend measurements](../development/backend-refactor-verification.md) report these separately.

The [IPC query states](../architecture/ipc-contract.md#query-session-states) define `ready`, `superseded`, and `stale`. The frontend stores or merges a current `ready` page, discards an obsolete local response, reports a still-current `superseded` start as a retryable query error while retaining the old cache, and starts a new query for a `stale` page.

The initial page size and later page limit are clamped by Rust to 1–256. Page offsets use unsigned transport values; an offset past the snapshot end is clamped to `total`. Summary lookup runs in chunks of 500 IDs and reconstructs the snapshot order after SQLite returns rows. The registered legacy `list_assets` command is not the production gallery path; it returns full `Asset` rows, clamps its limit to 1–500, and does not provide a stable session.

Query start, page reads, position lookups, and detail reads run as blocking work outside the async runtime. Command failures reject with a string as described in the [IPC contract](../architecture/ipc-contract.md).

## Mutation refresh and overlapping reads

`createMutations` uses `queryImpact.ts` to evaluate accepted tag results against the latest applied filter and refresh function. `none` preserves the query; `all` refreshes; `tags` refreshes for an include/exclude intersection or an actual count change under an exact-count filter. Lightbox and both bulk editing modes share this predicate. Missing-ID recovery and Favorites rules also use current query state.

`query.patchAssets` increments a local patch counter synchronously. Start/page calls capture it before IPC. If a ready response crosses a patch, the model rereads that page from the returned session before merging or returning rows to asynchronous callers. It repeats when another patch overlaps, retaining generation checks and request deduplication. These rereads preserve the query and thumbnail queue; unavailable sessions keep ordinary stale recovery. Query reads do not wait for metadata ownership. Favorite saves publish backend success and start any required refresh independently; refresh failure belongs to query recovery and does not reject the saved mutation.

## Frontend sparse page cache

The cache in `createAssetQuery` has three coordinated structures:

- `pages: Map<pageOffset, assetId[]>` records which IDs occupy each loaded page;
- `assetsById: Map<assetId, AssetSummary>` stores the summaries verbatim; and
- `lru: pageOffset[]` records the eviction order from the last merge; a bounded access-order array records subsequent explicit reads.

A merge replaces or adds the page, upserts its assets, and evicts the least recently accessed unprotected page until no more than 12 pages remain. Range access and async page/asset reads promote pages through a bounded local array without publishing reactive state. Render-time `getAssetAt` stays read-only. The latest gallery range protects up to 12 intersecting pages; if an oversized range needs more, the exact cache bound still wins. Eviction deletes the page and its asset objects and thumbnail retention removes inactive paths outside cached pages or active preview subscriptions.

`total` is the session's global match count and is the virtualizer count. By contrast, the returned `assets` array is only `Array.from(assetsById.values())`: it contains cached assets, is not padded to `total`, and must not be indexed as the global result list. `offset` is the greatest merged page end observed and is retained mainly for the reach-end loader.

Global-index access goes through these methods:

- `getAssetAt(index)` computes the aligned page offset, finds the ID at the local page position, and returns the cached asset or `undefined` for a hole.
- `getAssetAtAsync(index)` returns a cached asset or loads the aligned page, then returns that local position. Concurrent lookups for the same missing page await the same in-flight Promise and receive the same page result. Page-load failures reject; an unavailable position can resolve as `undefined`.
- `retryAssetAt(index)` explicitly clears only the aligned target page's failure and performs the same asynchronous lookup. Lightbox Retry uses it for offscreen or prefetched failures without restarting the query or unblocking other failed gallery pages. Ordinary demand remains blocked until explicit recovery.
- `getAssetIndex(assetId)` scans cached page ID arrays and returns the snapshot index only if that asset's page is loaded and a session is active; otherwise it returns `null`.
- `getAssetPosition(assetId)` uses the backend session's ordered ID vector to return `resolved` with an index, `missing` for an absent ID, or `stale` for an unavailable session. It neither loads pages nor transfers the ID vector. A stale backend response starts a refresh; a response from a superseded local generation is discarded. An unavailable current query rejects, leaving explicit retry to the caller.
- `getIdsRangeAsync(from, to)` loads every intersecting page and returns the inclusive global ID range. It rejects the whole request when a page is stale, cancelled, failed, or lacks any expected position; callers never receive a silently truncated range.

Only one request per page offset is started at a time. A local Promise map deduplicates callers, while a separate in-flight counter keeps `loading` true until all tracked query/page requests finish. Refresh clears the Promise map; an older request's `finally` removes its entry only if that exact Promise is still registered, so it cannot erase a newer-generation request for the same offset. A page response is discarded when its captured local generation is obsolete. The active session ID is captured before the request; no page is requested without a session or outside the current `total`. The frontend trusts a backend `ready` page's returned session and revision rather than comparing them again, because the backend page command is keyed by the supplied session ID.

## Summary rows and the details boundary

Sessions materialize `AssetSummary`, which carries display/order metadata and thumbnail/group fields but no byte size or tags. `preview_path` is populated for GIF/video summaries. Gallery tiles do not load original GIFs. The lightbox presents media only from a verified path, including a GIF/video summary preview path. Ordinary image summaries have no path and wait for complete details.

The page cache stores `AssetSummary` objects verbatim: nothing fabricates a path, byte size, or tag list, so a cache entry can never masquerade as a complete record. Gallery tiles read only summary fields (`file_name`, `preview_path`, thumbnail state, kind, media-group fields).

Selecting a tile opens the lightbox with a `SelectedAsset` view built from the summary: `path` starts as the summary's `preview_path` (a valid source for GIF/video; null for ordinary images), and `size_bytes` starts as null. When that path is null, the media stage renders a loading placeholder until details arrive, or a failure notice when the detail read fails. Existing GIF/video preview paths remain usable while complete details and tags load. `createLightbox` then loads through the shared details cache, which calls `getAssetDetails`; a successful `AssetDetails` replaces the view with the full path, size, and tags. Selection-request IDs prevent a late detail response from replacing a newer selection. Details for the previous and next global indices are also prefetched, using `getAssetAtAsync` when their pages are absent. A missing detail row or failed detail request leaves the summary view usable. See [lightbox](lightbox.md) for the editing and navigation behavior built on this boundary.

## Virtual range loading and thumbnail prefetch

`createVirtualGrid` gives TanStack Virtual the row count, `ceil(assetCount / columnCount)`, with one lane and a module-stable row-index key function. Each mounted row expands to its global asset indices. Virtual slots use global-index keys. A loaded asset replaces a placeholder inside its slot; a changed asset identity updates that slot and its thumbnail subscription. Scroll events, thumbnail updates, and page arrivals reuse existing row measurements instead of rebuilding a layout for every asset.

The grid measures its content width and its origin inside the actual scroll container. Tiles remain square with a 12-pixel gap and four rows of overscan on either side. The full-width search/tools header sits outside the gallery scroll container, which fills the remaining window height and owns the scrollbar below both header rows. Header wrapping adjusts that viewport through CSS layout. Bulk mode adds a 360px inspector at widths of at least 1000px; it sticks to the content viewport's top and scrolls independently within that viewport's height. The existing resize observers measure the narrower grid and retain the visible anchor. Changes to tile size or column count invalidate row measurements and keep the previous first visible asset in view. The scroll container, gallery, and grid have resize observation; listeners are removed on unmount.

The physical grid spacer is capped at 4,000,000 pixels by `scrollCoordinates.ts`,
below WebKit's layout-height limit. Virtual-core still measures the logical rows.
Its offset observer translates the physical scrollbar position to the logical offset;
mounted tiles and group backplates subtract the difference between those offsets.
The header margin and footer tail keep a one-to-one pixel mapping. Small collections
retain an identity mapping. Grid width, viewport height and tile-size changes rebuild
the mapping, with column/size changes preserving the first visible global index.
The adapter writes the new bounded height before restoring that anchor, so the old
spacer cannot clamp the new position.

On compressed grids, a nonpassive wheel listener converts pixel, line and page deltas
to logical pixels. Ctrl-wheel remains tile zoom; the bulk inspector keeps its own
scrolling. Subpixel logical offsets survive WebKit rounding of physical `scrollTop`.
Rectangle edge scrolling uses the same logical scroll command. The native scrollbar
still spans the whole result set. Listeners, observers and virtual-core subscriptions
are disposed with the gallery.

The gallery requires a scroll-container accessor. The virtualizer uses only that element and waits while the ref is null. An owned compute/apply effect starts observation after all required elements exist. A second effect configures the virtualizer and restores the global-index anchor when columns or tile size change. This also applies when Back from Settings remounts the gallery. Using the gallery as a temporary fallback would mistake its full content height for the viewport, mount every slot, and demand pages across the entire library.

The range callback reports `{ startIndex, endIndex, visibleStartIndex, visibleEndIndex }`, with inclusive indices. Pixel movement within unchanged row ranges updates tile placement without republishing page demand. `createLibrary` loads pages intersecting the overscan range and replaces gallery thumbnail demand with visible and prefetch IDs from currently loaded summaries. Range/query-epoch changes request pages. Page arrivals update thumbnail demand separately without requesting evicted pages again. The grid also reports its range when its failure/reset key changes. Failed offsets remain blocked in that generation until explicit Retry. Empty ranges clear gallery thumbnail demand. Bulk and duplicate preview requests remain additive and independent of this replaceable demand.

Sparse viewport demand is the primary production loader. `AppGalleryView` passes
`hasMore={library.query.offset < library.query.total}` and the query's `handleReachEnd`.
The two-row near-end threshold remains active when more rows are available and the
query is idle. `offset` is the greatest merged page end, so reaching the final sparse
page can set `hasMore` false while earlier pages are absent. Viewport demand still
loads those pages. Thumbnail progress renders outside the virtual-row computation,
so progress-counter changes alone do not invalidate its layout.

## Gallery presentation states

- If `assetCount` is zero, the grid shows an empty state. It distinguishes “no folders” from “no results” using the scan-root list and offers the add-folder action only for the former.
- If a virtual slot's page is not cached, the grid renders a square pulsing placeholder. Once the summary arrives, the slot becomes an interactive tile.
- Images and videos use a thumbnail when available; otherwise they use a transparent one-pixel placeholder. A preview load error also falls back to that placeholder. The per-tile spinner appears only while the thumbnail store reports that the asset is rendering and no thumbnail path is ready.
- Mounted virtual tiles load their previews eagerly. TanStack Virtual already bounds the mounted range, and native image lazy loading is avoided because WebKitGTK can fail to schedule absolutely positioned, translated virtual items.
- New gallery thumbnails stay transparent until loaded, then fade over the existing tile background with a 300 ms ease-in-out opacity animation. This includes cached images and tiles mounted during scrolling. A source change resets readiness; an unchanged-source rerender preserves it. Only the image participates in this fade, with no stagger or delay to queries, tile placement, controls, or selection feedback. Reduced-motion preferences show loaded images immediately. `ThumbnailImage` defaults to no fade outside the gallery.
- GIFs always use a static JPEG thumbnail or the placeholder, including while idle, scrolling, and after a preview error. Gallery tiles never request original GIF sources. Opening a GIF in the lightbox retains animation. Neutral, opaque corner chips identify videos and GIFs with readable text in both themes. Photos have no media-kind chip; tile borders use the shared theme border color.
- A nonblank media-group key marks the tile as grouped. Backplates join consecutive loaded virtual entries only when they share the exact key, are adjacent lanes, and sit on the same row; a group that crosses a row or unloaded gap is drawn as separate pieces. Different group keys also start separate pieces. Each decorative backing extends 4px around its tiles, with rounded corners behind the images and no pointer handling. The theme tokens `--gallery-group-bg` and `--gallery-group-edge` give dark mode a solid `#20382b` fill and a 1px inset `#52735c` edge. The inset preserves the gap between neighboring groups without changing tile geometry. Light mode keeps the primary color at 16% opacity with a transparent edge. Selection outlines and checkmarks retain the brighter primary color.
- The status footer reports thumbnail generation only when `hasMore` is true, and reports “End of results” with a locale-formatted, pluralized result count when thumbnail generation is idle and `hasMore` is false. It does not represent asset-page loading. The other two combinations render no footer content. `GalleryGrid` passes its required `assetCount`, the query total from `AppGalleryView`, as the footer's required `resultCount`; there is no cached-array-length fallback. The centered footer follows the grid in normal document flow, with roughly 136px of vertical space, flexible dividers, a decorative success checkmark, and wrapping text. It scrolls with the gallery without covering the last row; empty collections retain their empty state and have no footer.

The focused gallery and App tests cover totals, sparse caches, footer visibility, and locale changes. The desktop case in `e2e/specs/ui.redesign.e2e.js` selected by `--mochaOpts.grep 'footer'` checks scroll geometry, the final row, typography, narrow windows, and bulk mode in English and Polish under both themes. Run it with `MEDIATAGGER_UI_REDESIGN=1` and the [isolated desktop setup](../development/testing.md#animation-geometry). It saves synthetic screenshots and measurements under `artifacts/ui-redesign/`.

## Bulk rectangle selection

`createGridHandlers` owns the pointer gesture, capture, animation frame, and temporary highlights. `selectionGeometry` computes inclusive global-index ranges from the virtual grid's columns, square tile size, gap, and scroll position. Pointer coordinates add the current logical-minus-physical scroll offset before range
intersection; the screen rectangle subtracts it again. That offset changes while
scrolling and does not cancel the gesture as a layout change. It does not inspect
mounted tiles or summaries. Unloaded placeholders receive the same preview highlight as loaded tiles. The gallery interaction section fills its available height, including padding and unused space below the last row; the status footer does not intercept pointer input.

Holding the primary pointer for at least 150 ms and moving at least five pixels draws a translucent primary-color rectangle clipped to the visible gallery. An animation frame updates overlap and scrolls near the viewport's vertical edges. On release, the complete ranges go to `createSelection`, which resolves at most 128 indices per read, sequentially in query order, and commits selected IDs once. The preview remains while resolution is pending unless geometry changes make its position obsolete; bulk writes stay disabled until the read settles. Failure restores the previous visual selection and offers Retry. Layout changes cancel a held pointer gesture and remove its preview. Once the pointer is released, its global-index read and any failed Retry intent survive geometry changes, including error-banner insertion and listener re-registration. New input, query changes, disabled selection, cancellation, lost capture, blur, and unmount discard stale selection work. Escape clears selection and cancels pending gesture work without leaving bulk mode. The gallery registers a nonmodal UI layer so an open modal receives Escape first. See [bulk selection](search-tags-and-media-groups.md#bulk-selection) for click and modifier behavior.

## Refresh and invalidation paths

The dependency revision is authoritative for session invalidation. General changes and changes to a query's dependencies make its later reads `stale`, according to [database](database.md). The global revision alone does not invalidate a session. The frontend also refreshes eagerly on these paths:

| Trigger | Frontend behavior |
| --- | --- |
| Applied filters change | The search command calls `applyFilters`, which refreshes even for an identical descriptor. A ready first page replaces the old cache. |
| Scan/rescan, scan-root removal, CSV import, duplicate rename/delete | The settings workflow calls `refreshLibrary`, which refreshes assets and known tags together. Root removal first resets the thumbnail queue. |
| Database restore | The backend closes admission, drains admitted operations, closes idle pooled connections, and clears query sessions before replacement. The frontend invalidates tag/details identity state inside its mutation barrier, resets the thumbnail queue and map, refreshes roots, then refreshes assets and known tags. |
| Clear library | After backend success, the frontend immediately resets the thumbnail queue, thumbnails, cached assets, total, offset, and known tags. It does not need to query the now-empty library. |
| Lightbox delete | Removes the loaded object and selection locally, refreshes known tags, then starts a fresh asset query. |
| Bulk favorite toggle | Uses all selected IDs, patches loaded summaries and the shared details cache, and refreshes after either toggle direction in a favorites-only query or after missing IDs. Existing selected IDs survive the refresh. |
| Favorite toggle | Patches loaded state. Removing an item while favorites-only is applied also refreshes; other favorite changes do not. |
| Single/bulk tag edit | Patches loaded objects locally. Tag operations also refresh known tags. The returned impact restarts the query for repairs, changed names in the current include/exclude filter, or actual count changes under an exact-count filter. Unrelated edits preserve the session even when pages have been evicted. |
| Single/bulk media-group edit | Patches loaded objects locally and then always restarts the asset query, because group changes alter ordering and adjacency of the active view. |
| Thumbnail generation or cleanup | Updates thumbnail state without changing query membership/order and does not bump the library revision. The thumbnail store is authoritative for production tiles. |

Starting a refresh resets the active thumbnail queue but keeps the old asset cache and `total` visible until a current `ready` first page replaces them. Replacement clears both page IDs and `assetsById`, so rows retained only by an older session cannot leak into the new compact cache. Old start and page responses cannot merge after the local generation changes.

## Error behavior

An invoke or database error rejects the corresponding promise and records `loadError`. Failed page offsets are remembered per generation, so repeated renders, viewport demand, and direct reads cannot retry them automatically. Explicit Retry unblocks all failed pages in the current generation and reissues failed demanded pages within the same session. A pending selection retry loads its own offscreen ranges; a failed initial query restarts. A successful unrelated page cannot clear another page's failure. `stale` pages still restart the query automatically.

Inclusive range reads capture one session and generation, checking them after every awaited page. Refresh, library reset, and unmount invalidate pending reads. A changed session rejects the entire range before callers can apply a partial selection. Query loading counts are separate from settings-operation loading.

## Known limitations

- Backend supersession, TTL, and the four-session LRU are process-wide. Another window or independent caller can supersede an uncached start or evict this window's session.
- `assets` covers loaded pages only; bulk operations use authoritative selected IDs and a separate selection metadata cache. `getAssetIndex` cannot locate an unloaded asset. The 12-page bound takes precedence if an unusually large requested viewport intersects more than 12 pages.
- Lightbox and bulk use one shared 256-entry details cache, stamped with metadata tokens and cleared whenever the query epoch changes. Reselecting an asset after a restore, identity reset, or session restart re-reads details instead of trusting stale path/size/tags. Lightbox details carry metadata-generation stamps; complete snapshots from before or during a write are rejected, and current details are re-read after mutations drain.
- Refresh does not cancel in-flight backend page work; the generation guard only discards late responses after they finish. A `superseded` result is not retried automatically.
- The empty state depends only on `assetCount`, so it can appear during the initial load while `total` is still zero. Conversely, an in-progress refresh keeps the old nonempty grid visible. The footer does not expose asset-page loading. Its end state requires both idle thumbnail generation and `hasMore=false`.
- Global-index slot identity survives page changes; each slot still has to update its current asset and release its previous thumbnail subscription. Group backplates are presentation-only fragments for loaded entries in one row; they do not visually span row boundaries or holes.
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
10. Run Rust formatting and the relevant backend query/database tests, then run the focused Solid model, gallery, selection, search-filter, lightbox, and thumbnail tests. For cache or virtualizer changes, also exercise a library larger than 12 pages and scroll backward into evicted ranges.

### Relevant existing tests

- `src-tauri/src/db.rs` tests cover filter composition, grouped ordering, summary-supporting data invariants, cooperative ID-build cancellation, and related mutations including same-transaction revision bumps.
- `src-tauri/src/services/asset_query_service.rs` has a focused unit-test module covering equal-key session reuse, inverted admission, independent owner generations, capacity rejection/recovery, revision-stale pages, LRU/TTL eviction, clear semantics, and snapshot starts.
- `src/library/query/__tests__/createAssetQuery.test.ts` covers first-page replacement, current supersession error/retry and obsolete-response rejection, start/page failure error state with retry recovery, stale-triggered restarts, page-failure epoch signaling, and late-generation response rejection.
- `src-tauri/tests/backend_integration.rs` covers file-backed asset/tag/query flows, although it does not exercise `AssetQueryManager` session policy directly.
- `src/library/__tests__/createLibrary.test.ts` covers first-page replacement, filter forwarding, duplicate reach-end suppression, shared in-flight indexed page lookup, virtual-range thumbnail IDs, and clear-library reset.
- `src/features/gallery/__tests__/GalleryGrid.test.tsx` covers sparse-slot replacement, bounded DOM across cache eviction/reload, explicit page retry, static GIF previews and per-asset thumbnail DOM isolation. `createGridHandlers.test.tsx` covers pointer/modifier and cancellation behavior; `createVirtualGrid.test.tsx` covers ranges and measurements.
- `src/ui/controls/__tests__/ThumbnailImage.test.tsx` covers fade readiness for delayed/cached loads, failures, source replacement, unchanged-source rerenders, and previews with fading disabled. The opt-in [desktop thumbnail fade checks](../development/testing.md#gallery-thumbnail-fade) sample opacity during filtering and scrolling in both themes and with reduced motion.
- `src/features/lightbox/__tests__/createLightbox.test.ts` covers detail/mutation races, identity-reset ID reuse, deletion tombstones, query-session cache invalidation, rapid navigation, and query-position recomputation. See [lightbox tests](lightbox.md#tests) for remaining navigation/prefetch scenarios.
- `src/features/search/__tests__/createSearch.test.ts` covers applied-state transitions, meta-filter validation, and explicit refresh when filters are unchanged.

### Gallery performance verification

`src/features/gallery/__tests__/createVirtualGrid.test.tsx` runs the installed TanStack virtualizer with 10,000 numeric slots and asserts that scrolling and page/status updates do not rebuild row measurements. It also checks external-scroll offsets, partial rows, and resize anchoring. A 26,000-slot regression mounts initially null parent refs, bounds every intermediate commit and range callback on cold mount and Settings return, and checks scrolling, resize anchoring, observer cleanup, and real page-loader demand through owner disposal and remounting. These tests simulate DOM geometry in jsdom; they do not measure Linux desktop latency. Queue/store tests cover latest-viewport priority, additive consumers, reset/unmount races, atomic tile updates, and version cleanup; page-cache tests cross the 12-page bound and reload evicted pages.

The opt-in desktop spec uses 2,048 copied PNGs, 12 GIFs, and two videos in a temporary root. Its first case samples an image-only route before metadata traversal, then revisits the identical route without clearing caches. The starting target at index zero, the initially mounted area, and the final return legs are excluded from timing. Each sample records the final JPEG filenames present before scrolling and the visible thumbnail filenames after decoding. Only samples with no pre-existing visible previews enter the first-use summary; mixed samples stay in the raw record. Every revisit sample must find all visible previews already on disk. These labels describe preview-file reuse, not an OS-cold cache or a fully resident frontend page cache.

The suite crosses the 12-page cache bound, returns to evicted ranges, bounds mounted tiles, and compares decoded pixels from native-display captures of lightbox GIF frames. Run the entire suite with `MEDIATAGGER_GALLERY_PERF=1 bun run test:e2e:tauri --spec e2e/specs/gallery.performance.e2e.js` in the [isolated desktop environment](../development/testing.md#gallery-performance-measurements). The pixel check requires ImageMagick `import` and `magick`. Raw route samples, frame intervals, cache preparation and summaries go to `artifacts/gallery-performance/timings.json`. Timings include WebDriver overhead and exclude the opacity fade; they are observations, not portable frame-rate thresholds. The [native scale fixture](../development/testing.md#million-result-gallery-layout) verifies million-result layout separately with lazy synthetic summaries. Neither suite generates a million-file SQLite dataset.

The stable-metadata regressions cover unread pages, cached starts, position lookup, dependency changes, and file-backed WAL snapshots. Database tests cover marker-write rollback, initialization without markers, namespace rebuilding, exact batched keys, canonical no-ops, and shared-tag repairs. Backup tests restore both missing and obsolete marker namespaces. Frontend tests use pending saves and reads to check current-filter decisions, ownership rejection, and repeated page rereads after favorite saves. The opt-in `gallery.performance.e2e.js` case uses 2,048 temporary PNGs with a common tag, observes real IPC transport, and checks unchanged session IDs and scroll anchors across 16 pages and eviction. It also verifies reopened/reloaded edits and relevant tag/Favorites refreshes.
