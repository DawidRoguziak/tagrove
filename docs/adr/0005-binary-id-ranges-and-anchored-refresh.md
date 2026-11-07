# 0005: Binary ID ranges and anchored query refresh

- Status: Accepted
- Date: 2026-11-07
- Areas: IPC, library query, gallery selection
- Related: [IPC contract](../architecture/ipc-contract.md#binary-id-ranges), [library query and gallery](../subsystems/library-query-and-gallery.md), [plan stage 04](../plans/grid-performance/04-query-cache.md), [ADR 0003](0003-custom-gallery-virtualizer.md), [ADR 0004](0004-trigger-maintained-gallery-sort-keys.md)

## Context

A backend query session already holds the complete ordered ID vector of its filter (`src-tauri/src/services/asset_query_service.rs`). The frontend still reached that order only through 128-row summary pages:

- **Range selection.** Shift-click and rectangle selection resolved IDs with `getIdsRangeAsync`, which loaded every intersecting page sequentially, pushed it through the page cache LRU and republished the cache. A 100,000-item Shift-click cost about 780 page IPCs and seconds of work, and it evicted the viewport's pages.
- **Refresh.** `start_asset_query` always returned page 0. A refresh deep in the list (after a scan, CSV import or a mutation that changes membership) needed `start` and then `page` before the viewport could show rows again.

The scale target is 1,000,000 results. One million IDs as JSON is about 8–14 MB of text that the WebView must parse into a number array.

## Options considered

1. **Keep page-based ranges and only parallelize them.** Still O(n / 128) IPCs and SQL summary reads for data the session already has. Rejected.
2. **A JSON `number[]` range command.** One IPC, but JSON serialization and parsing dominate at 10^5–10^6 IDs. Rejected.
3. **A binary `tauri::ipc::Response` with raw i64, read through `BigInt64Array`.** Exact for every i64, but every ID needs a BigInt-to-number conversion. Rejected (user decision).
4. **A binary response with f64 values, read through `Float64Array` (chosen).** No conversion: the typed array is the result. Exact only up to 2^53, which SQLite row IDs created by this application never approach. The backend enforces the bound.
5. **Return several pages from an anchored start.** More data per refresh than the first visible page; the viewport's other pages are covered by a render-only fallback instead (see the library query page). Rejected.
6. **A `revision` argument on the range command.** Plan stage 04 sketched one. Rejected: sessions are already bound to their dependency revision, and `page`/`position` decide staleness the same way. A second revision check would disagree with them for unrelated global bumps.

## Decision

- **New command** `get_asset_query_id_range(sessionId, start, end)` (`src-tauri/src/commands/assets.rs`). It copies `asset_ids[start..=end]` (end clamped) from the session without SQL and returns a binary body: little-endian f64 `[count, ...ids]`. An empty body means `stale`; a ready empty range is eight bytes. An ID outside `0..=2^53` fails the whole command.
- **Frontend decoding** in `src/platform/tauri/assets.ts` (`decodeAssetIdRange`) validates the length and count header and returns `{ status: "ready", ids: Float64Array }` or `{ status: "stale" }`.
- **`getIdsRangeAsync`** (`src/library/query/createAssetQuery.ts`) reads ranges through the new command in chunks of at most 1,000,000 IDs. It never touches the page cache and keeps its contract: it rejects the whole range on staleness, a replaced session or a short result, and never truncates. `createSelection` makes one call per selection envelope.
- **Anchored start.** `start_asset_query` takes an optional `anchorOffset`. A `ready` result carries the aligned page that contains it (clamped to the last row) and reports its `offset`; without an anchor it is page 0, so the change is additive. `createLibrary` passes the first visible index.

## Consequences

- Shift-click across 100,000 items is one IPC plus a typed-array copy. The frontend share measured under jsdom is in the stage 04 notes.
- Selection reads no longer populate or evict the page cache, and their failures stay in the selection state (its own error and Retry). They no longer go into the page-failure set.
- The IPC contract has its first binary response. Tests mock it with `src/test/idRange.ts`, as described in [testing](../development/testing.md). Desktop fault injection in `e2e/specs/selection-recovery.e2e.js` targets the new command.
- If asset IDs ever exceed 2^53 (an external writer forcing huge row IDs), range selection fails visibly instead of returning wrong IDs. Switching to i64 and `BigInt64Array` would then need a new ADR.
- `start` keeps its old signature in Rust; `start_at` adds the anchor, so existing callers such as backup tests are unchanged.
