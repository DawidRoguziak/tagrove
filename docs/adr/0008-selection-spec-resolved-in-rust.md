# 0008: Bulk selection as a spec resolved by the backend

- Status: Accepted
- Date: 2026-11-07
- Areas: gallery selection, bulk metadata writes, IPC, write queue
- Related: [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md#bulk-selection), [IPC contract](../architecture/ipc-contract.md#asset-mutations-and-duplicate-operations), [plan stage 09](../plans/grid-performance/09-selection-spec.md), [ADR 0001](0001-per-asset-metadata-write-queue.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md), [ADR 0005](0005-binary-id-ranges-and-anchored-refresh.md), [ADR 0026](0026-set-based-ranged-group-write-with-index-rebuild.md), [ADR 0029](0029-range-selection-keeps-its-session-alive.md)

## Context

The bulk selection was a `Set<number>` of every selected ID (`createSelection.ts`). Every change copied the set, and bulk commands sent every ID as JSON. Selecting a 1,000,000-item result meant a million-entry set (tens of MB), a copy per change and a multi-megabyte payload per command. The backend already holds the ordered ID vector of every query session, so a selection made of ranges can be resolved where the IDs live.

## Options considered

1. **Keep ID sets, read them faster** (ADR 0005 binary ranges). Removes the page reads, not the memory or the payload. Rejected for large selections.
2. **Ranges plus small ID sets, resolved by the backend (chosen).** Constant frontend memory, O(log r) lookups, one small payload.
3. **Keep a range selection across query changes by re-resolving it.** A range means positions in the result the user saw; under another filter or after a reorder the same positions are other assets. Rejected: large range selections are cleared with a notice instead.
4. **Write a large selection in committed batches.** Shorter write-lock holds, but a revision change halfway through would leave a partial write, and the plan requires all or nothing. Rejected; instead the tag merge for a selection is set-based (below).
5. **Queue a selection write behind only the IDs it resolves to.** The frontend does not know them. Rejected: it takes a library-wide turn in the per-asset queue.

## Decision

- **Spec:** `{ sessionId, ranges: [start, end][], excluded: number[], included: number[] }`. Ranges are inclusive session positions, sorted and non-overlapping; `excluded` are unselected IDs inside them, `included` selected IDs outside them (possibly from an earlier query).
- **Frontend (`createSelection`):** `isSelected(index, id)` is a binary search over ranges plus a map lookup; `count` is the covered length − excluded + included. Shift-click, rectangle and Ctrl+A (select all) add ranges without reading IDs when they cover more than `MATERIALIZE_LIMIT` (10,000) positions; smaller ranges are resolved to IDs right away, as before, so small selections keep surviving filter changes. When the query session changes while ranges exist (a new filter, a scan, a reorder), the selection is cleared and the bulk panel says so. A cached session with the same ID keeps the ranges.
- **Command:** `apply_selection_mutation(selection, mutation)` with `mutation` one of `mergeTags`, `toggleFavorite`, `setMediaGroup` (`src-tauri/src/services/selection_service.rs`). The backend resolves the spec on the session vector in memory, then opens one IMMEDIATE user-write transaction, checks the session's dependency revision inside it, and only then writes and bumps the revision. A missing session or changed revision returns `{ status: "stale" }` and writes nothing. The existing ID commands remain for small selections.
- **Revision rule:** the spec carries the session ID, not the public revision. A session stays valid as long as its dependency revision is unchanged (the rule pages already use), so an unrelated tag write, including the selection's own previous tag write, does not invalidate it.
- **Tag merge for a selection is set-based:** IDs go into a temporary table and each tag is one `INSERT … SELECT`; `tag_count` is recounted exactly for assets that gained a tag. 1,000,000 assets: 2.8 s instead of 35 s for the per-asset merge. Unlike the per-asset merge it does not canonicalize legacy tag rows of every asset; that repair stays on the per-asset paths.
- **Write queue (ADR 0001):** a selection write takes a library-wide turn. It waits for every earlier queued write and every later write waits for it, so admission order still defines the graph and nothing can deadlock. Its turn invalidates all cached complete tag lists and details, because the affected IDs are unknown to the frontend.

## Consequences

- Select all, Shift-click and rectangle over 1,000,000 results are O(1) in memory and IPC; the counter and tile rings need no IDs.
- A large range selection does not survive a change of the query result. Grouping a range selection reorders the gallery, so a following write on the same spec is refused as stale and the selection is cleared.
- One selection write holds the SQLite write lock for its duration: about 2–3 s per 1,000,000 assets for tags or favorites. Background writers wait within their busy timeout; the frontend queue keeps the app's own writes behind it.
- The group order editor needs materialized IDs and is not offered for range selections; grouping a range assigns orders in gallery order.
- The command is additive; the ID-based bulk commands are unchanged.
