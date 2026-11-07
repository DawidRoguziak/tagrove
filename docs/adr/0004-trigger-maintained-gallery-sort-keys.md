# 0004: Trigger-maintained gallery sort keys with an exact filtered order

- Status: Accepted
- Date: 2026-11-07
- Areas: database schema, query semantics, scanning, CSV import, backup/restore
- Related: [database persistence](../subsystems/database.md), [library query and gallery](../subsystems/library-query-and-gallery.md), [data safety and portability](../subsystems/data-safety-and-portability.md), [plan stage 03](../plans/grid-performance/03-sort-key-and-indexes.md), [ADR 0002](0002-backend-write-priority-and-scan-revisions.md)

## Context

A query session holds the complete ordered ID vector of its filter. Every library revision change rebuilds that vector, and a scan bumps the revision up to once per second (ADR 0002).

The rebuild query joined the filtered rows to a per-group aggregate and sorted on `CASE`/`trim()`/`'group:' || key` expressions. SQLite ran it as `MATERIALIZE` plus two temporary B-trees, one for `GROUP BY` and one for `ORDER BY`. Release measurements (`src-tauri/tests/grid_baseline.rs`) on a synthetic 1,000,000-asset library:

- 368–395 ms with no filter;
- 664–673 ms when excluding a tag carried by 20% of assets.

The order has one property any change must keep. Group bucket statistics (newest `modified_at`, highest ID) are computed over the rows that pass the active filters (`ordered_asset_ids_filter_before_computing_group_stats`). A filtered view therefore places a group by its newest *matching* member.

Raw SQL writes `assets` in many places: test fixtures, the baseline seed, backup staging, and potentially an older application binary on the same database.

## Options considered

1. **Whole-group sort keys for every query.** All queries `ORDER BY` stored keys. This is simplest, but filtered views would place groups by members the filter hides. That changes visible order. Rejected by the user.
2. **Sort keys maintained by application code** in every write path: scan upsert, single and bulk group setters, CSV import, delete, cleanup, restore. Every raw-SQL writer, fixture, and older binary would bypass it and silently corrupt the order. Rejected.
3. **Keep the old query and add indexes only.** The `CASE` expressions and the filtered aggregate cannot use an index for ordering. Rejected.
4. **Trigger-maintained whole-group keys, a stored-key index for unfiltered queries, and an exact in-memory order for filtered queries (chosen).**

## Decision

**Columns** (`performance_schema_version` 4, `src-tauri/src/db/schema.rs`):

- `sort_bucket_time`, `sort_bucket_id`, and `sort_group_null_order` on `assets`.
- A row belongs to a group when its key is non-NULL and `trim(key) <> ''`. Bucket identity is `CAST(media_group_key AS TEXT)`, the same coercion as the former `'group:' || key`. A BLOB key and a TEXT key with the same bytes share one bucket.
- For a group member, the columns hold the whole group's newest `modified_at`, its highest ID, and `media_group_order IS NULL`. For a singleton they hold `(modified_at, id, 0)`.
- `sort_bucket_id` is therefore a unique integer bucket identity.

**Indexes:**

- `idx_assets_sort_key(sort_bucket_time DESC, sort_bucket_id DESC, sort_group_null_order, media_group_order, modified_at DESC, id DESC)`.
- Partial `idx_assets_group_bucket(CAST(media_group_key AS TEXT), modified_at)` on grouped rows.
- Five redundant indexes are dropped on every start:
  - `idx_tags_name`, `idx_asset_tags_asset`, `idx_asset_tags_tag`, and `idx_assets_kind` duplicated the leading columns of a unique key or another index;
  - `idx_assets_modified_at` served no query: the gallery order uses `idx_assets_sort_key`, and the deferred `list_assets` sorts by bucket expressions.

**Triggers.** Six triggers named `assets_sort_key_*` (`SORT_KEY_TRIGGERS`) maintain the columns:

- singleton insert and update set the row's own keys;
- grouped insert, update (old bucket and new bucket), and delete recompute the affected bucket, writing only rows whose keys change;
- the update triggers fire only on `UPDATE OF id, modified_at, media_group_key, media_group_order` when a value actually changes.

`init_schema` drops those trigger names before any write and recreates them from canonical SQL on every start.

**Bulk deferral.** `DeferredSortKeys` keeps bulk writers linear:

- A transaction-scoped marker row `library_metadata('sort_keys_deferred')` turns the grouped triggers off.
- The writer records each changed row's old and new key, then removes the marker and recomputes each touched bucket once before committing.
- It is used by `set_assets_media_group_bulk` and CSV import.
- A rollback removes the marker with the rest of the transaction.
- If a committed marker is ever found, `init_schema` deletes it and backfills all keys.

**Backfill.** For a database below version 4, `init_schema` computes every row's keys in one transaction and records version 4.

**Order query** (`try_list_ordered_asset_ids_with_meta`):

- **No filter:** `SELECT id FROM assets ORDER BY <idx_assets_sort_key columns>`. The plan is `SCAN assets USING COVERING INDEX idx_assets_sort_key`, with no temporary B-tree.
- **Filters:** SQL returns the matching rows with `(id, modified_at, sort_bucket_id, sort_group_null_order, media_group_order)` and no `ORDER BY`. Rust recomputes the newest matching time and highest matching ID per `sort_bucket_id`, then sorts by the legacy tuple: NULL group order first, numbers compared by value.
- Tag filters first resolve names to IDs. A missing or duplicate included tag yields an empty result, as the former `HAVING COUNT(DISTINCT t.id)` did. Exclusion is `a.id NOT IN (SELECT asset_id FROM asset_tags WHERE tag_id IN (…))`; `asset_id` is `NOT NULL`, so this equals `NOT EXISTS`.

The order is bit-for-bit the former order. This is proven against a verbatim copy of the old SQL for 19 filter combinations, on a hand-written fixture and a 5,000-row random fixture (`golden_order` in `src-tauri/tests/backend_integration.rs`). A randomized write sequence asserts the same after every 100 writes (`sort_key_maintenance`).

**Backup:**

- Validation accepts `performance_schema_version` 0–4.
- For version 4 it requires the three columns and keys consistent with a full recomputation (`count_inconsistent_sort_keys`).
- For every bundle version it rejects any view and any trigger outside `SORT_KEY_TRIGGERS`.
- `SCHEMA_VERSION`/`user_version` and the manifest `schema_version` stay 3, because the change is additive.

## Consequences

Measurements (release build, 1M assets; the 20k-file scan runs on a 100k library):

| Case | Before | After |
| --- | --- | --- |
| Rebuild, no filter | 368–395 ms | 31–35 ms |
| Exclude a 20% tag | 664–673 ms | 413–420 ms |
| Include a 20% tag | 226–233 ms | 101–105 ms |
| Favorites | 88–92 ms | 46–52 ms |
| 20k-file scan | 637–873 ms | 480–765 ms |
| Favorite write during scan, p50 | 4.3 ms | 2.0 ms |
| Upgrade backfill (`init_schema`) | — | ~1.5 s |
| Bulk group of 5,000 | (4.5–23 s without deferral) | 146–171 ms |

Other consequences:

- `media_group_order` comparisons follow SQLite numeric semantics in Rust: `-0.0` equals `0.0`, and NULL sorts first.
- Any statement that inserts or deletes `assets`, or updates the ordering columns, compiles the triggers into its prepared program. Hot paths must use `prepare_cached`. The scan upsert does: without caching, the 20k-file scan took 2.4–2.7 s.
- Writes that do not touch ordering columns (`tag_count`, `is_favorite`, `thumb_path`) do not fire the triggers.
- Ad hoc SQL that writes only the derived columns can still make them inconsistent. Backup validation detects this; the running application has no periodic check.
- An older binary on a version-4 database keeps the keys correct, because the triggers live in the database. It does not use them, and it rejects a version-4 bundle with "backup has incompatible Tagrove metadata", because it accepts only versions 0–3.
- Removing the `list_assets` IPC command was deferred: 17 desktop E2E specs and several Rust tests still use it as an assertion helper. Its separate query is unchanged.
