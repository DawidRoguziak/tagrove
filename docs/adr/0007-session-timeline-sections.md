# 0007: Timeline sections of query sessions

- Status: Accepted
- Date: 2026-11-07
- Areas: library query, IPC, gallery
- Related: [library query and gallery](../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber), [IPC contract](../architecture/ipc-contract.md#asset-queries-and-reads), [plan stage 08](../plans/grid-performance/08-timeline-scrubber.md), [ADR 0003](0003-custom-gallery-virtualizer.md), [ADR 0004](0004-trigger-maintained-gallery-sort-keys.md), [ADR 0027](0027-unfiltered-sections-unavailable-instead-of-stale.md) (amends the row-count rule)

## Context

At 1,000,000 results the gallery scrollbar is a poor way to reach a given month: one pixel of track covers more than a thousand items. Plan stage 08 adds a month timeline ("scrubber"). It needs, for each month, the session position where the month starts, and it must not add a blocking pass to the query build (Immich #31360 is the cautionary example).

The gallery order (ADR 0004) is descending by bucket time: the newest `modified_at` of the bucket (a media group, or the asset alone), over matching members when filters apply. Every member of a bucket shares that time.

## Options considered

1. **A separate SQL aggregation per section request** (`GROUP BY month`). Month arithmetic in SQL for every row, and with filters it must repeat the bucket statistics the build already computes. Rejected.
2. **Sections in the ID build, labelled by each asset's own `modified_at`.** A grouped asset from another month would split its group between two sections, and the same month could appear twice. Rejected.
3. **Sections labelled by the bucket time (chosen).** One O(n) pass over rows that are already sorted. The month never increases along the vector, so a month is one contiguous run, a group is never split, and no month can appear twice; adjacent equal labels are only the deliberate splits below.
4. **Always in the ID build.** Measured first: reading `sort_bucket_time` and `sort_bucket_id` for every row of the unfiltered index scan doubled the 1M rebuild (29 → 58 ms of SQL streaming, rebuild 32 → 67 ms), while the in-memory builder costs only 1.5 ms. Rejected for the unfiltered path.
5. **Local-time months.** The backend has no time-zone database, and sessions are shared by key across callers. A UTC offset argument would make the section list depend on the caller. Rejected for now; see consequences.
6. **Replace the windowed `GalleryScrollbar` with the timeline.** The bar is the only position control when a result has one month or no dates, and its row and page keys serve a different purpose. Rejected: the two coexist.

## Decision

- Sections are `{ t: yyyymm, start, count }`, built by `SectionBuilder` (`src-tauri/src/db/query_sections.rs`). A **filtered** build produces them in the same pass as its IDs, from the bucket statistics it already computed. The **unfiltered** build reads only IDs; its sections are read on the first `get_asset_query_sections` call (`list_unfiltered_query_sections`, `sort_bucket_time` only, same covering index and order) inside the read transaction that checks the session's revision, and stored in the session. Members of a bucket share their time, so splitting where the time changes never splits a group.
- `t` is the UTC calendar month of the bucket time. A time at or before the Unix epoch is "no date" (`t = 0`); it sorts last.
- A month larger than 5,000 rows is split into further sections with the same `t`, only between buckets, so one group is never split.
- The session stores its sections once known. The new command `get_asset_query_sections(sessionId)` returns `ready` with `session_id`, `total` and the list, or `stale` under the same dependency-revision rule as pages. It is JSON: a realistic library has a few hundred months.
- The frontend loads sections once per session, merges split months for labels, and maps a track position to an index as `index / total`; the grid turns an index into a row offset in O(1). The timeline is a `role="slider"` beside the position bar (native, or `GalleryScrollbar` when windowed). A drag holds page and thumbnail demand until release or 150 ms at rest.

## Consequences

- Costs (release, three `grid_baseline` runs): the unfiltered rebuild stays at 33–34 ms for 1M (3.9 ms for 100k); the first sections read costs 28.5–28.8 ms for 1M (2.7 ms for 100k), off the first-page path because the frontend reads sections after the `ready` start; cached reads 0.02 ms. The filtered builder adds about 1.5 ms per 1M rows; the month arithmetic is cached per month because computing it per row cost a quarter of the rebuild.
- The unfiltered sections are read in a transaction that first checks the session's dependency revision, and their row count must equal the session total; otherwise the read is `stale`.
- Months are UTC. An asset modified near midnight at a month boundary can be labelled with the neighbouring month in local time. Changing this needs a session key that includes the offset.
- The command is additive; no existing payload changes.
- Grid section headers would need prefix sums over variable row heights and are not part of this decision.
