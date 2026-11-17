# Library query and gallery

Implementation entry points: [query manager](../../src-tauri/crates/media_tagger_core/src/services/asset_query_service.rs), [query use cases](../../src-tauri/crates/media_tagger_core/src/usecases/assets.rs), [page cache](../../src/library/query/createAssetQuery.ts), [range loading](../../src/library/createLibrary.ts), [virtual grid](../../src/features/gallery/GalleryGrid.tsx).

This page owns query-session caching, frontend page loading/invalidation, and virtual gallery behavior. It explains how applied search state becomes a revision-bound query and a bounded sparse page cache.

The transport shapes and command-registration rules are in the [IPC contract](../architecture/ipc-contract.md). SQL filters, grouped ordering, and the library revision are described in [database](database.md). Related user workflows belong in [search, tags, and media groups](search-tags-and-media-groups.md), [thumbnails](thumbnails.md), and [lightbox](lightbox.md).

## Frontend ownership

`src/library/createLibrary.ts` composes the shared models under
`createAppRuntime`; feature views do not construct separate library owners.
`query/createAssetQuery.ts` preserves 128-item pages, sparse positions, a 24-page in-place cache,
render-only refresh fallback, debounced stale recovery, failed offsets, explicit retry, patch-counter rereads and stale-session recovery.
Operational state changes synchronously, while getters publish through Solid. Separate
memos prevent loading-only changes from republishing the cached asset array. Lightbox commands
use `navigationSnapshot()` for the synchronous epoch, pending state and total, so a key press
in the same batch as refresh cannot use the previous session.

Viewport or query-epoch changes request pages. Page arrivals publish the viewport's rows,
whose tiles then request their `thumb://` thumbnails, without requesting evicted pages again. This keeps oversized ranges
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

`features/gallery/createVirtualGrid.ts` is the gallery's own virtualizer over the pure
geometry in `gridGeometry.ts` ([ADR 0003](../adr/0003-custom-gallery-virtualizer.md)). It waits for
measured grid width before publishing nonempty ranges, owns observers under the mounted view
and preserves global-index resize anchors. The empty-result state unmounts the grid content
and clears its element, so the virtual grid and the selection handlers drop their observers
instead of measuring a detached node. Geometry is published from inside a ResizeObserver
callback, so no observer is created in response to it: the selection handlers' observer
follows only the grid and scroller elements, and a new one in that delivery would raise
WebKit's "ResizeObserver loop" error. `GalleryGrid` renders a fixed pool of row slots;
crossing a row re-indexes one slot, so thumbnail loads, page arrivals and scrolling
create no tile components. Each loaded tile points its image at its current asset's `thumb://` URL.
A tile reads its slot's last shown asset rather than the narrowed `<Show>` accessor, because
WebKit still answers a recycled tile's detached image after the slot moved to an unloaded
index, and reading the accessor then would throw "Stale read from <Show>".
The image element keeps its identity and resets load and failure state when its source changes.
Gallery images have no entrance animation.

The current tests cover the geometry and model at 10,000, 26,000 and 1,000,000 slots, drive
sparse replacement, explicit page retry and eviction/reload across the 24-page bound, and
exercise the native WebView with synthetic collections. See the
[browsing desktop recipe](../development/testing.md#solid-feature-verification).

## From applied filters to a session

The top bar has editable filter state and separate applied filter state. A valid submit copies the input, media kind, and favorites flag into the applied state. `createSearch.submit` passes the normalized descriptor to `createLibrary.applyFilters`, which refreshes the query. Every valid submission refreshes, including one whose descriptor is unchanged. Invalid search metatags do not change the applied state or start a query.

Production uses a page size of 128. `createAssetQuery.refresh`:

1. cancels any pending stale-driven refresh, increments its local generation and query epoch, marks the query pending, and invalidates navigation positions;
2. clears the active session reference and in-flight offset set;
3. enters loading state; thumbnails are unaffected, because every summary carries its own `thumb_version`;
4. calls `startAssetQuery` with the applied filters, owner ID, generation, page size, and `anchorOffset` = the latest gallery range's first visible index (`createLibrary` supplies it), so the `ready` result already carries the viewport's page;
5. accepts only a `ready` response for the still-current local generation; and
6. advances the query epoch again and replaces the frontend page cache with the returned page at its returned offset. Rows of the previous viewport's other pages stay visible as [render-only fallback](#frontend-sparse-page-cache) until those pages reload.

Each query model creates a random owner ID for its lifetime. The local generation rejects late frontend responses. Native async admission can run out of call order, so the backend retains the highest admitted generation per owner for the process lifetime. An equal or lower generation gets an obsolete token without replacing the current request. A new owner starts with an independent generation counter. Accepted registrations still share one process-wide request token. Filter normalization, the owner-ID check (1–128 bytes) and this registration run in `usecases::assets::begin_asset_query_service` before the `start_asset_query` command spawns its blocking task, so they keep the order in which commands are admitted; the session start then runs on the blocking pool. Input normalization belongs to [IPC validation](../architecture/ipc-contract.md#boundary-validation-and-normalization-summary), SQL membership to [database filters](database.md#filter-semantics), and user syntax to [search grammar](search-tags-and-media-groups.md#search-grammar-and-filter-semantics).

## Backend session and page contract

`AssetQueryManager` compares owner generations under an admission mutex before scheduling blocking work; one authoritative registration token defines the runtime's latest accepted request. It reads the global and dependency revisions, builds the ordered ID list, and materializes the first page inside one deferred read transaction, so a ready response reflects a single SQLite snapshot. On a cache miss it asks SQLite for the complete ordered list of matching asset IDs, using a SQLite progress callback during execution and sorting as well as row-loop checks so a superseded request stops work. That frozen ID vector, rather than full rows, is the session snapshot. The ordering keeps matching media-group members adjacent, orders group buckets by their newest matching member, and then applies group order, modification time, and ID tie-breakers. Without filters the list is a covering scan of trigger-maintained sort keys. With filters, Rust recomputes the matching-member bucket statistics and sorts in memory (see [database ordering](database.md#gallery-and-media-group-ordering) and [ADR 0004](../adr/0004-trigger-maintained-gallery-sort-keys.md)). The cancel check also runs before and after that sort.

Each session stores its dependency keys and their maximum revision. Only a changed dependency makes its pages or position lookups stale. Unrelated tags and favorites preserve IDs, order, total, and the cached session ID, while the public `revision` advances with the read snapshot. Dependency keys and initialization rules belong to [database revisions](database.md#library-revision-and-query-sessions).

The application-owned cache has these bounds:

- at most four sessions and 64 MiB of retained ID-vector capacity;
- expiry after five minutes without cache access (any lookup by key or session ID restarts it; a held range selection reads one ID of its session when it is made and then every minute so it does not expire, [ADR 0029](../adr/0029-range-selection-keeps-its-session-alive.md));
- most-recently-accessed promotion for lookup by key or session ID; and
- reuse of an equal normalized-filter/dependency-revision key, including reuse of its session ID.

A separate cache epoch changes on clear. Insertion checks both the request token and epoch under the cache mutex, preventing an in-flight build from repopulating a cleared cache. SQLite progress callbacks are removed before connection reuse. An authoritative session larger than the retained-ID budget rejects with a text error asking the user to narrow the search. It does not return `superseded`, evict existing sessions, or retry automatically. Empty and populated frontend views expose the query error and Retry; a new filter submission can recover with a fitting query. The retained-ID budget excludes transient SQLite allocations and vectors still held by in-flight readers.

The [IPC query states](../architecture/ipc-contract.md#query-session-states) define `ready`, `superseded`, and `stale`. The frontend stores or merges a current `ready` page, discards an obsolete local response, reports a still-current `superseded` start as a retryable query error while retaining the old cache, and starts a new query for a `stale` page.

The initial page size and later page limit are clamped by Rust to 1–256. Page offsets use unsigned transport values; an offset past the snapshot end is clamped to `total`. Summary lookup pads each chunk to a cached 16-, 64-, or 256-ID statement and reconstructs the snapshot order after SQLite returns rows. The registered legacy `list_assets` command is not the production gallery path; it returns full `Asset` rows, clamps its limit to 1–500, and does not provide a stable session.

A filtered build also produces the session's [timeline sections](#timeline-sections-and-scrubber) in the same pass; an unfiltered session reads them on the first sections request, so the rebuild stays an ID-only index scan.

Query start, page reads, position lookups, section reads, and detail reads run as blocking work outside the async runtime. Command failures reject with a string as described in the [IPC contract](../architecture/ipc-contract.md).

## Mutation refresh and overlapping reads

`createMutations` uses `queryImpact.ts` to evaluate accepted tag results against the latest applied filter and refresh function. `none` preserves the query; `all` refreshes; `tags` refreshes for an include/exclude intersection or an actual count change under an exact-count filter. Lightbox and both bulk editing modes share this predicate. Missing-ID recovery and Favorites rules also use current query state.

`query.patchAssets` increments a local patch counter synchronously. Start/page calls capture it before IPC. If a ready response crosses a patch, the model rereads that page from the returned session before merging or returning rows to asynchronous callers. It repeats when another patch overlaps, retaining generation checks and request deduplication. These rereads preserve the query; unavailable sessions keep ordinary stale recovery. Query reads do not wait for metadata ownership. Favorite saves publish backend success and start any required refresh independently; refresh failure belongs to query recovery and does not reject the saved mutation.

## Frontend sparse page cache

The cache in `createAssetQuery` has three coordinated structures, mutated in place:

- `pages: Map<pageOffset, assetId[]>` records which IDs occupy each loaded page;
- `assetsById: Map<assetId, AssetSummary>` stores the summaries verbatim; and
- `lru: pageOffset[]` records the eviction order from the last merge; a bounded access-order array records subsequent explicit reads.

A merge replaces or adds the page in place, upserts its assets, and evicts the least recently accessed unprotected page until no more than `MAX_CACHED_PAGES` (24) pages remain. It never copies the maps. Every cache change bumps one `version` signal; render-time readers (`getAssetAt`, the `assets` memo, and through them the `assetAt` memo of each pooled grid cell) read `version` so they rerun after an in-place change. Range access and async page/asset reads promote pages through a bounded local array without publishing reactive state. Render-time `getAssetAt` stays read-only. The latest gallery range protects up to 24 intersecting pages; if an oversized range needs more, the exact cache bound still wins. Eviction deletes the page and its asset objects; thumbnails need no retention because tiles address them by ID and version.

When a refresh replaces the cache, the rows of the latest gallery range's other cached pages move into a render-only fallback map. Only `getAssetAt` reads it: `getAssetIndex`, `getAssetAtAsync`, `getIdsRangeAsync`, `loadPage`, and `assets` see only the current query. A fallback page disappears when the new query loads that offset, when the gallery range no longer includes it, on the next refresh or reset, or when it starts at or beyond the new `total`. A refresh deep in the list therefore never blanks its viewport. A fallback tile can briefly show an old row at an index whose new row differs; clicking it resolves the asset through `getAssetPosition`, because `getAssetIndex` does not know it.

`total` is the session's global match count and is the virtualizer count. By contrast, the returned `assets` array is only `Array.from(assetsById.values())`: it contains cached assets, is not padded to `total`, and must not be indexed as the global result list.

Global-index access goes through these methods:

- `getAssetAt(index)` computes the aligned page offset, finds the ID at the local page position, and returns the cached asset, a refresh fallback row for that position, or `undefined` for a hole.
- `getAssetAtAsync(index)` returns a cached asset or loads the aligned page, then returns that local position. Concurrent lookups for the same missing page await the same in-flight Promise and receive the same page result. Page-load failures reject; an unavailable position can resolve as `undefined`.
- `retryAssetAt(index)` explicitly clears only the aligned target page's failure and performs the same asynchronous lookup. Lightbox Retry uses it for offscreen or prefetched failures without restarting the query or unblocking other failed gallery pages. Ordinary demand remains blocked until explicit recovery.
- `getAssetIndex(assetId)` scans cached page ID arrays and returns the snapshot index only if that asset's page is loaded and a session is active; otherwise it returns `null`.
- `getAssetPosition(assetId)` uses the backend session's ordered ID vector to return `resolved` with an index, `missing` for an absent ID, or `stale` for an unavailable session. It neither loads pages nor transfers the ID vector. A stale backend response schedules a [debounced refresh](#refresh-and-invalidation-paths); a response from a superseded local generation is discarded. An unavailable current query rejects, leaving explicit retry to the caller.
- `getIdsRangeAsync(from, to)` returns the inclusive global ID range as a `Float64Array` read from the backend session vector through the binary [`get_asset_query_id_range`](../architecture/ipc-contract.md#binary-id-ranges) command, one IPC per ≤ 1,000,000 IDs (`ID_RANGE_CHUNK`). It loads no pages and never touches the page cache or its LRU. It rejects the whole request when the session is stale (also scheduling the debounced refresh), replaced, or the read fails or returns fewer IDs than requested; callers never receive a silently truncated range.

Only one request per page offset is started at a time. A local Promise map deduplicates callers, while a separate in-flight counter keeps `loading` true until all tracked query/page requests finish. Refresh clears the Promise map; an older request's `finally` removes its entry only if that exact Promise is still registered, so it cannot erase a newer-generation request for the same offset. A page response is discarded when its captured local generation is obsolete. The active session ID is captured before the request; no page is requested without a session or outside the current `total`. The frontend trusts a backend `ready` page's returned session and revision rather than comparing them again, because the backend page command is keyed by the supplied session ID.

## Summary rows and the details boundary

Sessions materialize `AssetSummary`, which carries display/order metadata and thumbnail/group fields but no byte size or tags. `preview_path` is populated for GIF/video summaries. Gallery tiles do not load original GIFs. The lightbox presents media only from a verified path, including a GIF/video summary preview path. Ordinary image summaries have no path and wait for complete details.

The page cache stores `AssetSummary` objects verbatim: nothing fabricates a path, byte size, or tag list, so a cache entry can never masquerade as a complete record. Gallery tiles read only summary fields (`file_name`, thumbnail state, kind, media-group fields). A tile image's text alternative is the `file_name`, never the full `preview_path` of a GIF or video.

Selecting a tile opens the lightbox with a `SelectedAsset` view built from the summary: `path` starts as the summary's `preview_path` (a valid source for GIF/video; null for ordinary images), and `size_bytes` starts as null. When that path is null, the media stage renders a loading placeholder until details arrive, or a failure notice when the detail read fails. Existing GIF/video preview paths remain usable while complete details and tags load. `createLightbox` then loads through the shared details cache, which calls `getAssetDetails`; a successful `AssetDetails` replaces the view with the full path, size, and tags. Selection-request IDs prevent a late detail response from replacing a newer selection. Details for the previous and next global indices are also prefetched, using `getAssetAtAsync` when their pages are absent. A missing detail row or failed detail request leaves the summary view usable. See [lightbox](lightbox.md) for the editing and navigation behavior built on this boundary.

## Virtual range loading and thumbnail prefetch

Square tiles make every position arithmetic. `gridLayout` derives columns, tile size, stride
and `ceil(assetCount / columnCount)` rows from the measured width; `rowRange` maps an offset to
visible rows plus four rows of overscan on either side, all in O(1). `GalleryGrid` renders
`ceil(viewport / stride) + 1 + 8` row slots. Slot `s` shows the row congruent to `s` modulo the
pool, positioned by one 2D `translate` per row (`contain: strict`, a box padded by half a gap so
backplates and selection rings are not clipped); a CSS grid places the cells inside it. Slots
are ordered by row, so DOM order follows visual order for focus and screen readers; crossing a
row moves two offscreen row nodes. A cell reads `getAssetAt(index)`: a loaded asset renders a
tile, an uncached one a placeholder, and a changed asset updates that tile and its thumbnail
URL without remounting it.

The grid measures its content width and its origin inside the actual scroll container only
from resize observers; the scroll path reads nothing but `scrollTop`, and the row range signal
changes only when the first, last or visible rows change. Tiles remain square with a 12-pixel gap. The full-width search/tools header sits outside the gallery scroll container, which fills the remaining window height and owns the scrollbar below both header rows. Header wrapping adjusts that viewport through CSS layout. Bulk mode adds a dock floating over the bottom of the viewport instead of a column, so the grid keeps its width; `.workspace-body--bulk` adds 88px of bottom padding after the grid so the last row can scroll out from under the collapsed chip. Changes to tile size or column count invalidate row measurements and keep the previous first visible asset in view. The scroll container, gallery, and grid have resize observation; listeners are removed on unmount.

The physical grid spacer is capped at `MAX_GRID_HEIGHT` = 16,000,000 pixels. WebKitGTK 2.54
keeps layout, scroll offsets, transforms, hit testing and painting exact only below 2^24 CSS
pixels at device scales 1 and 2; above it odd offsets round and translated rows stop painting
(measured by `e2e/gallery-scale/height-probe.tsx`). Content up to that height scrolls natively
with the spacer as its full height and a window base of zero. Taller content scrolls natively
inside a physical window that starts at the logical offset `base`; rows render at
`rowTop - base`, and `scrollShift` exposes `base` to rectangle selection. Wheel, touchpad
inertia, PageUp/PageDown and the arrow keys stay native and one-to-one. The base moves, with
one `scrollTop` write in the same frame as the row positions, after 150 ms without motion
when the viewport is outside the middle half of the window, or immediately when native
scrolling comes within two viewports of a window edge the base could still move past.
Home/End reach the ends of the whole result: in this mode the window listener handles them
for the gallery, the scroller and the body (a recycled row can drop focus to the body), but
not for text controls or the bulk dock.

In the windowed mode the scroller carries `data-gallery-windowed`; `styles.css` hides its
native bar, which would span only the window, and `createVirtualGrid` pads the removed gutter
back so the column count does not change. `GalleryScrollbar` replaces it: a sticky
`role="scrollbar"` named “Gallery position” with `aria-valuenow` set to the first visible
item, thumb drag, a track press that jumps to the pointer, and Arrow/Page/Home/End keys.
Grid width, viewport height and tile-size changes preserve the first visible global index;
the spacer height is written before that anchor, so the old spacer cannot clamp it. While the
viewport top is still above the grid (including the first width measurement after a mount),
column changes keep the physical position instead, so the margin stays visible. A
margin-only change, such as the error banner, keeps `scrollTop` and moves the grid. Column
count, tile size, margin, pool size and spacer height are memos, so effects keyed on them
(such as the rectangle-selection handlers, which cancel a drag on layout changes) do not
rerun when only the rendered rows change. Writes are
clamped in JavaScript because WebKitGTK resets `scrollTop = Number.MAX_SAFE_INTEGER` to zero.

### Timeline sections and scrubber

Each session has timeline sections `{ t, start, count }` (`src-tauri/crates/media_tagger_core/src/db/query_sections.rs`,
[ADR 0007](../adr/0007-session-timeline-sections.md)). A filtered build produces them with its
IDs. An unfiltered build reads only IDs (reading the bucket columns per row doubled the 1M
rebuild); its sections are read once, on the first `get_asset_query_sections` call, from the
sort-key index inside the read transaction that checks the session's revision (about 29 ms for
1M rows, release), then kept in the session. A scan commits batches between its throttled
revision bumps ([ADR 0002](../adr/0002-backend-write-priority-and-scan-revisions.md)), so the live
index can hold rows the session snapshot does not while the revision still matches. When the
sections' row count differs from the session total, the read returns `ready` with an empty list,
keeps the session (its pages, positions, ID ranges and cached start stay valid), and stores
nothing; the scan's next bump (throttled, final or error-path) makes the session `stale`, and the
session built after it reads its sections again
([ADR 0027](../adr/0027-unfiltered-sections-unavailable-instead-of-stale.md)). A batch that only
changes existing rows' times keeps the count, so those sections can be slightly misaligned for
the session's remaining life, which the same bump ends.
`t` is the UTC month (`yyyymm`) of the row's **bucket time**, the time the gallery order sorts
by ([database ordering](database.md#gallery-and-media-group-ordering)): a media group belongs to
the month of its newest (matching) member, so a group is never split and a month is one
contiguous run. Times at or before the Unix epoch form a final "no date" section (`t = 0`).
A month larger than 5,000 rows is split between buckets (unfiltered: where the bucket time
changes) into further sections with the same `t`. `get_asset_query_sections` returns them for a session, or `stale` like a page read.

`createAssetQuery` loads sections once per session after a `ready` start (a cache hit with the
same session ID keeps them) and exposes `sections`; a failed or stale read, or a `ready` read
with no sections, leaves the timeline hidden. Only a stale read schedules a refresh. `GalleryTimelineScrubber` appears when the result spans at least two months. It is a
sticky `role="slider"` named “Timeline”, 10 px wide and 24 px from the right edge, beside the
position bar: GTK's overlay scrollbar (native mode) and `GalleryScrollbar` (windowed mode) take the
pointer hits of the right 14 to 21 px (the native one measures 21 px in the WebKitGTK E2E build;
a rail at 16 px lost its right half to it, so a click on its centre scrolled the page instead).
The gallery section reserves the rail (offset plus width, 34 px) as its right padding, so no tile
column lies under it and a click or rectangle start at a last-column tile's right edge reaches the
tile; the grid width, and so the column count, is measured after that padding.
It does not replace `GalleryScrollbar`
(that bar is the only position control for results with one month or no dates). Its
`aria-valuetext` is the month of the first visible item, formatted with `Intl.DateTimeFormat`
in the UI language and UTC. The track position of an index is `index / total`; a jump calls
`scrollToIndex`, whose row offset is O(1) with fixed row heights. A mouse press focuses the slider explicitly (`focus({ preventScroll: true })`), because its
`pointerdown` calls `preventDefault` and WebKitGTK then skips the default focus change; the keys
below therefore act on the rail after a click. Arrow keys step one month,
PageUp/PageDown one year (to the start of the current month or year first), Home/End the ends.
A drag applies at most one jump per animation frame and holds page and thumbnail demand
(`holdDemand`) until pointer release or 150 ms without movement (`resumeDemand`). Year ticks
are thinned so labels never overlap; hovering shows the month under the pointer. The grid has
no section headers, so row heights stay fixed.
Ctrl-wheel remains tile zoom; the gallery never intercepts plain wheel input. Subpixel
logical offsets survive WebKit rounding of physical `scrollTop`, and rectangle edge
scrolling uses the same logical scroll command. Listeners, observers, timers and the
windowed attribute are removed with the gallery.

The gallery requires a scroll-container accessor. The virtualizer uses only that element and waits while the ref is null. An owned compute/apply effect starts observation after all required elements exist. A second effect sets the spacer height, restores the global-index anchor when columns or tile size change, and moves the window when the result shrinks below its base. This also applies when Back from Settings remounts the gallery. Using the gallery as a temporary fallback would mistake its full content height for the viewport, mount every slot, and demand pages across the entire library.

The range callback reports `{ startIndex, endIndex, visibleStartIndex, visibleEndIndex, paused }`, with inclusive indices, or `null` when the gallery unmounts. Pixel movement within unchanged row ranges updates tile placement without republishing page demand, unless the pause state changes. `createLibrary` loads pages intersecting the overscan range. Thumbnail demand is not tracked there: each mounted tile requests its own `thumb://` URL, and the backend renders newest requests first ([thumbnails](thumbnails.md#the-thumb-protocol)). Range/query-epoch changes request pages. The grid also reports its range when its failure/reset key changes. Failed offsets remain blocked in that generation until explicit Retry.

Fast scrolling pauses automatic gallery loading, not rendering. The model measures absolute
distance in logical gallery pixels, including direction reversals, rectangle edge scrolling
and drags of the windowed scrollbar. A single move of at least half the viewport height, or a full viewport within
100 ms, starts the pause. Every position change extends it; duplicate scroll notifications
do not. After 150 ms without movement the same range is republished with `paused: false`,
even if its indices have not changed. Geometry changes and tile-size anchor writes reset
the motion history without starting a pause. Unmount cancels the timer.

During the pause, `createLibrary` remembers the latest range but skips automatic page
reads, including on query-epoch changes. Page arrivals can populate the cache but cannot
restart page demand while paused. Tiles that receive a new asset during the pause request a
cached thumbnail only (`c=1`): an existing file shows at once, a missing one is rendered only
after the pause ends. A tile whose overscan prefetch (`p=1`) is still pending when it reaches
the view keeps that URL during the pause and drops `p=1` once it ends, which moves its queued
render ahead of every prefetch ([thumbnails](thumbnails.md#frontend-tiles)). Resume loads only the latest range, retaining the four-row overscan.
Already dispatched reads can finish; caches remain intact. Explicit lightbox/selection reads
continue. Unmount publishes
`null` to release gallery demand while the application-owned library stays alive.

Sparse viewport demand is the only production loader: there is no sequential
"load more" or near-end callback, so jumping anywhere, including the end, loads only the
pages that intersect the range. The footer below the rows shows only the end-of-results
state; there is no thumbnail progress counter.

## Gallery presentation states

- If `assetCount` is zero, the grid shows an empty state. It distinguishes “no folders” from “no results” using the scan-root list and offers the add-folder action only for the former.
- During fast scrolling, tiles whose summaries are cached stay mounted with their thumbnails and backplates; page demand pauses and newly shown assets ask for cached thumbnails only. Placeholders for uncached slots stop pulsing while paused.
- If a virtual slot's page is not cached, the grid renders a square placeholder (`div[data-asset-index]`, pulsing outside the pause). Once the summary arrives, the slot becomes an interactive tile.
- Images and videos load their `thumb://` thumbnail. A load error is retried twice with backoff and then falls back to a transparent one-pixel placeholder; while the tile stays mounted with the same asset it keeps retrying with a backoff capped at 30 s, and an asset that comes back to a recycled tile is requested again ([thumbnails](thumbnails.md#frontend-tiles)). The per-tile spinner is CSS only: it appears 300 ms after a tile starts waiting for its image and disappears when the image loads or fails.
- Mounted virtual tiles load their previews eagerly with `decoding="async"`. The row pool already bounds the mounted range, and native image lazy loading is avoided because WebKitGTK can fail to schedule translated virtual items.
- Gallery thumbnails display at full opacity as soon as the browser loads them, including cached images and tiles remounted after scrolling. There is no entrance fade. A 300 ms fade previously left newly arriving rows dim during moderately fast scrolling and delayed visibility after the fast-scroll pause. A source change keeps the image element, resets load and failure handling, and hides the previous picture until the new source has loaded (or is already complete), so a recycled tile never shows another asset's thumbnail. The shared `ThumbnailImage` retains its opt-in fade for other consumers; the gallery does not request it.
- GIFs always use a static JPEG thumbnail or the placeholder, including while idle, scrolling, and after a preview error. Gallery tiles never request original GIF sources. Opening a GIF in the lightbox retains animation. Neutral, opaque corner chips identify videos and GIFs with readable text in both themes. Photos have no media-kind chip; tile borders use the shared theme border color.
- A nonblank media-group key marks the tile as grouped. Each row computes its own backplates: they join consecutive loaded cells only when they share the exact key and sit in adjacent lanes; a group that crosses a row or unloaded gap is drawn as separate pieces. Different group keys also start separate pieces. Each decorative backing extends 4px around its tiles, with rounded corners behind the images and no pointer handling. The theme tokens `--gallery-group-bg` and `--gallery-group-edge` give dark mode a solid `#20382b` fill and a 1px inset `#52735c` edge. The inset preserves the gap between neighboring groups without changing tile geometry. Light mode keeps the primary color at 16% opacity with a transparent edge. Selection outlines and checkmarks retain the brighter primary color.
- The status footer always reports “End of results” with a locale-formatted, pluralized result count. It does not represent asset-page loading. Because it follows the last row, the end state is only visible at the bottom of the result. `GalleryGrid` passes its required `assetCount`, the query total from `AppGalleryView`, as the footer's required `resultCount`; there is no cached-array-length fallback. The centered footer follows the grid in normal document flow, with roughly 136px of vertical space, flexible dividers, a decorative success checkmark, and wrapping text. It scrolls with the gallery without covering the last row; empty collections retain their empty state and have no footer.

The focused gallery and App tests cover totals, sparse caches, footer visibility, and locale changes. The desktop case in `e2e/specs/ui.redesign.e2e.js` selected by `--mochaOpts.grep 'footer'` checks scroll geometry, the final row, typography, narrow windows, and bulk mode in English and Polish under both themes. Run it with `MEDIATAGGER_UI_REDESIGN=1` and the [isolated desktop setup](../development/testing.md#animation-geometry). It saves synthetic screenshots and measurements under `artifacts/ui-redesign/`.

## Bulk rectangle selection

`createGridHandlers` owns the pointer gesture, capture, animation frame, and temporary highlights. `selectionGeometry` computes inclusive global-index ranges from the virtual grid's columns, square tile size, gap, and scroll position. Pointer coordinates add the current logical-minus-physical scroll offset before range
intersection; the screen rectangle subtracts it again. That offset changes while
scrolling and does not cancel the gesture as a layout change. It does not inspect
mounted tiles or summaries. Unloaded placeholders receive the same preview highlight as loaded tiles. The gallery interaction section fills its available height, including padding and unused space below the last row; the status footer does not intercept pointer input.

Holding the primary pointer for at least 150 ms and moving at least five pixels draws a translucent primary-color rectangle clipped to the visible gallery. An animation frame updates overlap and scrolls near the viewport's vertical edges. On release, the complete ranges go to `createSelection`. Ranges covering at most 10,000 positions are resolved with one binary ID-range read of their envelope (`getIdsRangeAsync`) and committed as IDs; larger ones are stored as ranges of the session without any read ([selection spec](search-tags-and-media-groups.md#bulk-selection), [ADR 0008](../adr/0008-selection-spec-resolved-in-rust.md)). The preview remains while resolution is pending unless geometry changes make its position obsolete; bulk writes stay disabled until the read settles. Failure restores the previous visual selection and offers Retry. Layout changes cancel a held pointer gesture and remove its preview. Once the pointer is released, its global-index read and any failed Retry intent survive geometry changes, including error-banner insertion and listener re-registration. New input, query changes, disabled selection, cancellation, and unmount discard stale selection work, and lost capture or window blur cancel a held gesture; a released read and its failed Retry intent survive blur. Escape clears selection and cancels pending gesture work without leaving bulk mode. The gallery registers a nonmodal UI layer so an open modal receives Escape first. See [bulk selection](search-tags-and-media-groups.md#bulk-selection) for click and modifier behavior.

## Refresh and invalidation paths

The dependency revision is authoritative for session invalidation. General changes and changes to a query's dependencies make its later reads `stale`, according to [database](database.md). The global revision alone does not invalidate a session. The frontend also refreshes eagerly on these paths:

| Trigger | Frontend behavior |
| --- | --- |
| Applied filters change | The search command calls `applyFilters`, which refreshes even for an identical descriptor. A ready first page replaces the old cache. |
| Scan/rescan, scan-root removal, CSV import, duplicate rename/delete | The settings workflow calls `refreshLibrary`, which refreshes assets and known tags together. |
| Database restore | The backend closes admission, drains admitted operations, closes idle pooled connections, and clears query sessions before replacement. The frontend invalidates tag/details identity state inside its mutation barrier, refreshes roots, then refreshes assets and known tags. Restored rows carry their own `thumb_version`. |
| Clear library | After backend success, the frontend immediately resets cached assets and refresh fallback, total, and known tags. It does not need to query the now-empty library. |
| Lightbox delete | Removes the loaded object and selection locally, refreshes known tags, then starts a fresh asset query. |
| Bulk favorite toggle | Uses all selected IDs, patches loaded summaries and the shared details cache, and refreshes after either toggle direction in a favorites-only query or after missing IDs. Existing selected IDs survive the refresh. A range selection (spec) always refreshes, because the affected rows are unknown to the frontend; an unchanged session keeps its ID and the ranges. |
| Favorite toggle | Patches loaded state. Removing an item while favorites-only is applied also refreshes; other favorite changes do not. |
| Single/bulk tag edit | Patches loaded objects locally. Tag operations also refresh known tags. The returned impact restarts the query for repairs, changed names in the current include/exclude filter, or actual count changes under an exact-count filter. Unrelated edits preserve the session even when pages have been evicted. |
| Single/bulk media-group edit | Patches loaded objects locally and then always restarts the asset query, because group changes alter ordering and adjacency of the active view. A range selection is then cleared with a notice, because its session changed. |
| Selection spec write `stale` | Nothing was written; the query refreshes, and the new session clears the range selection with a notice. If that refresh fails, the range selection is cleared with the "expired" notice instead ([bulk selection](search-tags-and-media-groups.md#bulk-selection)). |
| Thumbnail generation or cleanup | Updates thumbnail files and paths without changing query membership/order and does not bump the library revision. Tiles do not need a refresh: their URLs are stable per source version. |

Starting a refresh keeps the old asset cache and `total` visible until a current `ready` page replaces them. Replacement clears both page IDs and `assetsById`, so rows retained only by an older session cannot leak into the new compact cache; the old viewport's other pages survive only as render-only fallback. Old start and page responses cannot merge after the local generation changes.

A `stale` page, position, or reread response does not refresh immediately. It schedules one refresh `STALE_REFRESH_DELAY_MS` (250 ms) later; further stale responses join that pending refresh, and two stale-driven refreshes are at least `STALE_REFRESH_MIN_INTERVAL_MS` (1 s) apart. Any explicit refresh (filters, settings workflows, mutations, lightbox delete) runs at once and cancels the pending one. The backend already limits scan revision bumps to about one per second ([ADR 0002](../adr/0002-backend-write-priority-and-scan-revisions.md)); the debounce protects the frontend against bursts from any source.

## Error behavior

An invoke or database error rejects the corresponding promise and records `loadError`. Failed page offsets are remembered per generation, so repeated renders, viewport demand, and direct reads cannot retry them automatically. Explicit Retry unblocks all failed pages in the current generation and reissues failed demanded pages within the same session. Selection range failures are not page failures: they stay in the selection's own error state, and its retry repeats the original ID-range read. A failed initial query restarts. A failed refresh leaves the query without a session (`sessionId()` is `null`) and advances `pageFailureEpoch`; a range selection reads that pair as a lost session and is cleared with its "expired" notice, while a failed page read keeps the session. A successful unrelated page cannot clear another page's failure. `stale` pages still restart the query automatically, after the stale debounce.

Inclusive range reads capture one session and generation, checking them after every awaited range chunk. Refresh, library reset, and unmount invalidate pending reads. A changed session rejects the entire range before callers can apply a partial selection. `keepSessionAlive(sessionId)` is the selection's keepalive: one `0..=0` range read of that session, skipped when it is no longer the current one (resolves `"skipped"`); a `stale` reply schedules the debounced refresh and resolves `"stale"`, so the selection can drop ranges whose first read found the session already gone, and its errors stay with the caller ([ADR 0029](../adr/0029-range-selection-keeps-its-session-alive.md)). Query loading counts are separate from settings-operation loading.

## Known limitations

- Backend supersession, TTL, and the four-session LRU are process-wide. Another window or independent caller can supersede an uncached start or evict this window's session. Without a range selection nothing keeps the current session alive: the first page read after more than five idle minutes is `stale` and rebuilds it (the old rows stay visible as fallback until the new pages arrive).
- `assets` covers loaded pages only; bulk operations use authoritative selected IDs (or a selection spec for large ranges) and a separate selection metadata cache.
- Timeline months are UTC; an asset modified near midnight at a month boundary can appear under the neighbouring local month. `getAssetIndex` cannot locate an unloaded asset or a refresh fallback row. The 24-page bound takes precedence if an unusually large requested viewport intersects more than 24 pages.
- Lightbox and bulk use one shared 256-entry details cache, stamped with metadata tokens and cleared whenever the query epoch changes. Reselecting an asset after a restore, identity reset, or session restart re-reads details instead of trusting stale path/size/tags. Lightbox details carry metadata-generation stamps; complete snapshots from before or during a write are rejected, and current details are re-read after mutations drain. A range-selection write drops every cached tag list when it ends, so an open lightbox re-reads its asset's details then ([lightbox](lightbox.md#summary-to-details-selection-lifecycle)).
- Refresh does not cancel in-flight backend page work; the generation guard only discards late responses after they finish. A `superseded` result is not retried automatically.
- The empty state depends only on `assetCount`, so it can appear during the initial load while `total` is still zero. Conversely, an in-progress refresh keeps the old nonempty grid visible. The footer does not expose asset-page loading or thumbnail generation.
- Global-index slot identity survives page changes; each slot still has to update its current asset and, with it, its image `src`. Group backplates are presentation-only fragments for loaded entries in one row; they do not visually span row boundaries or holes.
- The connection pool bounds each acquisition at five seconds but aggregate retry loops can still wait longer; `db_pool.rs` tests that ordinary checkouts leave the user-write reserve free, but not the five-second timeout failure itself.

## Safe change checklist

1. Keep the Rust response enums, serialized snake-case fields, TypeScript unions, wrappers, and registered commands synchronized. Update the [IPC contract](../architecture/ipc-contract.md) for any transport or state change.
2. Preserve one canonical filter normalization and the complete SQL ordering tuple. Check tag AND/NOT, favorite, kind, exact tag-count, exact normalized group-name, group bucket, null group order, modification time, and ID tie-break behavior in [database](database.md).
3. Decide whether every write changes query membership or order. Bump the library revision on the backend and either refresh immediately or deliberately document a local patch that preserves all relevant membership and ordering invariants.
4. If page size or cache limits change, validate a positive frontend page size, keep backend clamps explicit, test boundary offsets, and prove the frontend never retains more than the intended exact page limit.
5. Treat `total` and cached `assets` as different quantities. Use global-index accessors for virtualized navigation and selection; do not index the loaded-assets array as though it were the session snapshot.
6. Keep `AssetSummary` lean. If a gallery feature needs full path, size, or tags, either justify adding that cost to every summary or cross the `AssetDetails` boundary and define cache invalidation for the added data.
7. Preserve generation checks, per-offset request de-duplication, loading-counter balance, and stale recovery. Add explicit cancellation or error state before relying on either behavior in the UI.
8. Test virtual holes, cache eviction and reload, resize/overscan ranges, placeholder-to-tile transitions, overscan thumbnails requested with `p=1` and the one-way drop of `p=1` when a pending overscan tile scrolls into view, zero-result loading, group backplates, and static GIF previews with missing, ready, and failed thumbnails.
9. Add backend tests for equal-key reuse, supersession, revision mismatch, missing/expired/evicted sessions, LRU promotion, TTL, page clamps, and cross-session isolation. Use a controllable clock or cache constructor rather than sleeping for the TTL.
10. Run Rust formatting and the relevant backend query/database tests, then run the focused Solid model, gallery, selection, search-filter, lightbox, and thumbnail tests. For cache or virtualizer changes, also exercise a library larger than 24 pages and scroll backward into evicted ranges.

### Relevant existing tests

- `src-tauri/crates/media_tagger_core/src/db.rs` tests cover filter composition, grouped ordering, summary-supporting data invariants, cooperative ID-build cancellation, and related mutations including same-transaction revision bumps.
- `src-tauri/crates/media_tagger_core/src/services/asset_query_service.rs` has a focused unit-test module covering equal-key session reuse, inverted admission, independent owner generations, capacity rejection/recovery, revision-stale pages, LRU/TTL eviction, the TTL restart by one-ID keepalive reads of a selected session, clear semantics, and snapshot starts.
- `src/library/query/__tests__/createAssetQuery.test.ts` covers first-page replacement, current supersession error/retry and obsolete-response rejection, start/page failure error state with retry recovery, debounced stale-triggered restarts, page-failure epoch signaling, late-generation response rejection, `version` notification of in-place merges, the page bound, render-only refresh fallback, and the session keepalive read (current session only, `stale` → debounced refresh).
- `src-tauri/tests/backend_integration.rs` covers file-backed asset/tag/query flows, selection specs resolved on a session (`selection_spec`), and in `sections_during_a_scan` the session policy of timeline sections across a real scan: a session kept through an unbumped batch commit, then stale after the scan's final bump and replaced by one with sections.
- `src/library/__tests__/createLibrary.test.ts` covers first-page replacement, filter forwarding, duplicate range-demand suppression, shared in-flight indexed page lookup, the paused-range hold, and clear-library reset.
- `src/features/gallery/__tests__/GalleryGrid.test.tsx` covers sparse-slot replacement, bounded DOM across cache eviction/reload, explicit page retry, static GIF previews, per-asset thumbnail DOM isolation, loaded tiles kept through a fast jump and through a refresh deep in the list, one-slot recycling with constant DOM and row order, per-row backplates and the windowed scrollbar. `gridGeometry.test.ts` covers layout, ranges, anchors and window bases at 10k/26k/1M slots; `createVirtualGrid.test.tsx` covers ranges, anchors, the windowed mode, the pause and disposal; `GalleryScrollbar.test.tsx` covers its accessibility, pointer and keyboard input. `timeline.test.ts` and `GalleryTimelineScrubber.test.tsx` cover month merging, position mapping, keys, ARIA, the focus taken on a press and the per-frame drag with held demand; `GalleryGrid.test.tsx` covers the timeline beside the windowed bar and the right padding that clears its rail; `query_sections.rs` and `asset_query_service.rs` tests cover section boundaries, groups, no-date rows, splits, filters and staleness, and that an unbumped scan commit keeps the session without sections until the next bump (also `backend_integration`, `sections_during_a_scan`, with a real scan). `createGridHandlers.test.tsx` covers pointer/modifier and cancellation behavior, including a released read and its Retry surviving window blur.
- `src/ui/controls/__tests__/ThumbnailImage.test.tsx` covers one element across source changes, hiding until the current source loads, async decoding, fade readiness for delayed/cached loads, failures, unchanged-source rerenders, and previews with fading disabled. The opt-in [desktop thumbnail checks](../development/testing.md#gallery-thumbnail-fade) require immediate full opacity during filtering and scrolling in both themes and with reduced motion.
- `src/features/lightbox/__tests__/createLightbox.test.ts` covers detail/mutation races, identity-reset ID reuse, deletion tombstones, query-session cache invalidation, rapid navigation, and query-position recomputation. See [lightbox tests](lightbox.md#tests) for remaining navigation/prefetch scenarios.
- `src/features/search/__tests__/createSearch.test.ts` covers applied-state transitions, meta-filter validation, and explicit refresh when filters are unchanged.

### Gallery performance verification

`src/features/gallery/__tests__/createVirtualGrid.test.tsx` mounts 10,000 and 26,000 slots with initially null parent refs, bounds every range callback on cold mount and remount, and checks that demand is republished only when rows change, resize and margin anchoring, partial rows, observer and timer cleanup, and the one-million-slot window (End/Home, edge and idle rebasing, subpixel edge scrolling, return to native mode). These tests simulate DOM geometry in jsdom; they do not measure Linux desktop latency. `GalleryGrid.test.tsx` checks the `thumb://` URL per slot, `p=1` on overscan tiles and its one-way drop when a pending overscan tile scrolls into view (kept for a loaded picture and during a pause), and `ThumbnailImage.test.tsx` covers per-source load state and retry backoff; page-cache tests cross the 24-page bound and reload evicted pages.

The opt-in desktop spec uses 3,584 copied PNGs (28 pages), 12 GIFs, and two videos in a temporary root. Its first case samples an image-only route before metadata traversal, then revisits the identical route without clearing caches. The starting target at index zero, the initially mounted area, and the final return legs are excluded from timing. Each sample records the final JPEG filenames present before scrolling and the visible thumbnail filenames after decoding. Only samples with no pre-existing visible previews enter the first-use summary; mixed samples stay in the raw record. Every revisit sample must find all visible previews already on disk. These labels describe preview-file reuse, not an OS-cold cache or a fully resident frontend page cache.

The suite crosses the 24-page cache bound (28 pages), asserts that a revisited page is fetched again after eviction, returns to evicted ranges, bounds mounted tiles, and compares decoded pixels from native-display captures of lightbox GIF frames. Run the entire suite with `MEDIATAGGER_GALLERY_PERF=1` in the [isolated desktop environment](../development/testing.md#gallery-performance-measurements), on a private Xvfb display exported as `MEDIATAGGER_NATIVE_DISPLAY` (the capture guard refuses any other display). The pixel check requires ImageMagick `import` and `magick`. Raw route samples, frame intervals, cache preparation and summaries go to `artifacts/gallery-performance/timings.json`. Timings include WebDriver overhead and exclude the opacity fade; they are observations, not portable frame-rate thresholds. The [native scale fixture](../development/testing.md#million-result-gallery-layout) verifies million-result layout separately with lazy synthetic summaries. Neither suite generates a million-file SQLite dataset.

The stable-metadata regressions cover unread pages, cached starts, position lookup, dependency changes, and file-backed WAL snapshots. Database tests cover marker-write rollback, initialization without markers, namespace rebuilding, exact batched keys, canonical no-ops, and shared-tag repairs. Backup tests restore both missing and obsolete marker namespaces. Frontend tests use pending saves and reads to check current-filter decisions, ownership rejection, and repeated page rereads after favorite saves. The opt-in `gallery.performance.e2e.js` case uses 3,584 temporary PNGs with a common tag, observes real IPC transport, and checks unchanged session IDs and scroll anchors across 28 pages and eviction. It also verifies reopened/reloaded edits and relevant tag/Favorites refreshes.
