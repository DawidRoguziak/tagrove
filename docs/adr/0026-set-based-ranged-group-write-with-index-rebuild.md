# 0026: A ranged media-group write is set-based, writes the new bucket's ordering keys itself, and rebuilds the group indexes when it changes a large part of the library

- Status: Accepted
- Date: 2026-11-11
- Areas: database (media groups, sort keys, indexes), bulk selection, write-lock contention
- Related: [database](../subsystems/database.md#gallery-and-media-group-ordering), [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md#bulk-selection), [ADR 0004](0004-trigger-maintained-gallery-sort-keys.md), [ADR 0008](0008-selection-spec-resolved-in-rust.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md)

## Context

`setMediaGroup` on a range selection (Ctrl+A in bulk mode, then Apply group or Remove groups) goes through `apply_selection_mutation` (`src-tauri/src/services/selection_service.rs`). It used `set_assets_media_group_bulk_in_tx`, which runs one read and one update per asset inside the single IMMEDIATE transaction (audit record `S-034d0ea6`). ADR 0008 made only the tag merge set-based. Background writers yield to a user write for at most 250 ms and then wait within their 5 s busy timeout, so a long write delays scans and thumbnails and, past about 20 s (four attempts), fails them.

Measured with `ranged_set_media_group_baseline` in `src-tauri/tests/grid_baseline.rs` (release build, disk-backed temporary database, 10% of rows in four-item groups), the longest stretch a concurrent writer was locked out was 13.8 s for grouping 999,997 of 1,000,000 assets, 9.9 s for clearing them, 1.7 s for a tenth of the library and 0.74 s for all of a 100,000-row library.

A set-based rewrite alone did not help. With the IDs staged in a temporary table, the single `UPDATE` still took about 6 s per 1,000,000 changed rows with a 256 MiB page cache and 12 to 25 s with the default 64 MiB one. Almost all of that is maintaining three indexes per row: `idx_assets_group_normalized`, `idx_assets_group_bucket` and `idx_assets_sort_key`. Each costs about 2 µs per changed row even with a warm cache, and more once the dirty pages outgrow the cache. The deferred bucket recompute (`DeferredSortKeys::finish`) then rewrote `idx_assets_sort_key` a second time for every new member (about 4 s). Triggers cost about 0.3 s, and processing the rows in index order made the random table access slower than what it saved.

## Options considered

1. **Per-asset statements (before).** About 14 µs per row under the lock. Rejected.
2. **Set-based statements that keep maintaining the indexes.** About 9 s at 1,000,000 rows. Rejected on its own, but kept for writes that change fewer rows.
3. **A larger page cache for the transaction.** Halves option 2 at the cost of a few hundred MB, and is still above the busy budget. Rejected.
4. **Committed batches.** Rejected in ADR 0008 because the write must be all or nothing.
5. **Refuse or cap large ranged group writes.** Takes away a working feature. Rejected.
6. **Drop the three group-write indexes, update, and recreate them in the same transaction (chosen for large writes).** Recreating one by sorting costs about 0.5 to 0.7 s per 1,000,000 rows, independent of how many rows changed.
7. **Drop and recreate the sort-key triggers as well.** Saves about 0.3 s, but adds a second schema rewrite on the user path. Rejected.

## Decision

- `apply_selection_mutation` with `setMediaGroup` calls `db::set_selection_media_group_in_tx` (`src-tauri/src/db/mutations.rs`). It stages the resolved IDs in ID order, with their one-based selection positions as orders, in `temp.selection_group_ids`, keeping only existing IDs. It then drops the rows whose key and order already match, using the per-asset path's comparison (`IS`, BINARY text, numeric order). An empty remainder returns without a write or a bump.
- The new bucket's `sort_bucket_time` and `sort_bucket_id` are computed once over its final members (its current members plus the changed rows) and written to its members outside the selection through `idx_assets_group_bucket`, and to the changed rows in the same `UPDATE` that sets key, order and normalized key. A cleared or blank key writes singleton keys. `DeferredSortKeys` keeps the bucket triggers off and recomputes only the buckets the changed rows leave.
- When at least `GROUP_INDEX_REBUILD_MIN_ROWS` (20,000) rows change, and at least an eighth of the library, the `UPDATE` runs inside `with_group_write_indexes_rebuilt` (`src-tauri/src/db/schema.rs`). That drops the three indexes, runs the update, recreates them with the exact SQL `init_schema` uses (shared constants), puts their `sqlite_stat1` rows back and reloads them with `ANALYZE sqlite_schema`. The old-bucket recompute runs after the rebuild, because it needs the bucket index.
- The revision is bumped once, in the same transaction, after the recompute. The ID-based `set_assets_media_group_bulk` command keeps the per-asset path.

## Consequences

- Measured with the same diagnostic on a busy host, the longest lock-out at 1,000,000 rows is about 3.7 to 4.1 s for grouping all assets, 3.2 to 3.4 s for clearing them, about 1 s for a tenth (maintained indexes), and 0.3 s for all of a 100,000-row library. A full-library change at 1,000,000 rows is now below the 5 s busy budget, with about 1 s to spare. Recreating the indexes (about 1.8 s) and the commit (about 0.4 s) are the remaining floor.
- Not covered: when the write raises a bucket's newest `modified_at` or highest ID, or removes the member that held it, the bucket's members outside the selection get new ordering keys row by row, through `idx_assets_sort_key` and outside the rebuild window. This applies to the new bucket before the update and to the old buckets in `DeferredSortKeys::finish`. The per-asset path paid the same. The cost scales with the bucket's size, not with the selection. For example, adding newer assets to an existing group of 1,000,000 members costs about 4 s for that rewrite alone. The diagnostic does not measure it.
- A large ranged group write changes the schema cookie. Every pooled connection re-prepares its statements once on next use, and long-lived readers keep their snapshot as usual. On an error the whole transaction rolls back, indexes and statistics included. Backup validation checks triggers and views, not index text, and the recreated indexes have the same stored SQL.
- New invariant: the three index definitions live in `GROUP_WRITE_INDEXES`. Any change to them goes there, so `init_schema` and the rebuild cannot drift apart.
- The new bucket's keys are written by application code instead of the canonical recompute. `ranged_group_write_matches_the_per_asset_path_with_and_without_an_index_rebuild` (`src-tauri/src/db.rs`) compares every group and ordering field with its SQLite type, the index SQL, the statistics and the revision with the per-asset path, on both index paths, over a mixed fixture and nine steps. `a_rebuilding_group_write_holds_the_write_lock_from_its_first_statement` uses `test_support::commit_revision_bump_before_every_statement` to show that no concurrent commit lands inside the write and that it bumps once. `selection_spec` in `src-tauri/tests/backend_integration.rs` checks the service path against the per-asset path on a small library and a 24,000-row library that takes the rebuild path.
- ADR 0008's statement that tag and favorite spec writes hold the lock for 2 to 3 s per 1,000,000 assets still holds. Group writes are now in the same range when they change most of the library.
