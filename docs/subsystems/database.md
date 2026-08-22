# Database persistence

This page is the canonical description of MediaTagger's current SQLite schema and query semantics. The implementation in `src-tauri/src/db.rs`, the query services, and executable tests remain authoritative. For startup, profile isolation, and workflow locking, see the [system overview](../architecture/system-overview.md); for command validation, payloads, and query-session result states, see the [IPC contract](../architecture/ipc-contract.md). Workflow details belong in the narrower subsystem pages for [scanning and indexing](scanning-and-indexing.md), [library queries and the gallery](library-query-and-gallery.md), [search, tags, and media groups](search-tags-and-media-groups.md), and [data safety and portability](data-safety-and-portability.md), rather than here.

## Current guarantees

### Database location and connection policy

The application stores `media.db` directly in the effective Tauri profile's app-data directory. Development, E2E, and release identifiers therefore use separate databases. Startup acquires that profile's `instance.lock` before opening the database, creates the app-data and thumbnail directories, calls `open_connection`, and runs `init_schema`. Initialization rejects a nonzero foreign `application_id` and a future `user_version`; after success it records MediaTagger's `application_id` (`0x4d544147`) and schema version (`1`). The path is retained in `AppState`; commands normally open their own connection to it.

Every connection created through `db::open_connection` applies:

| Setting | Current value and effect |
| --- | --- |
| `journal_mode` | `WAL`; committed pages may be in `media.db-wal`, with `media.db-shm` coordinating WAL readers. Both sidecars are live database state, not disposable cache files. |
| `synchronous` | `NORMAL`; WAL commits trade the strongest power-loss durability for lower sync overhead. |
| `foreign_keys` | `ON` on every application-created connection, enabling the declared cascades. |
| `temp_store` | `MEMORY`. |
| `cache_size` | `-65536`, approximately 64 MiB per connection because a negative value is in KiB. |
| `mmap_size` | `268435456` bytes (256 MiB requested). |
| Busy timeout | Five seconds. A lock still held after that becomes an operation error. |

`init_schema` finishes with `PRAGMA optimize`; a non-empty scan also runs it after its revision bump. Every normal read/write or read-only connection owns a process-wide maintenance lease. Bundle export closes that gate and uses SQLite Backup API to create one standalone `media.db` snapshot; it does not archive WAL or SHM. Bundle inspection and restore validate the extracted database as described in [data safety and portability](data-safety-and-portability.md). Restore checkpoints staging after migration and again after path rewriting, checkpoints the current database, and removes sidecars only after their pages are durable in the corresponding main file.

Gallery query sessions use the process-wide pool in `services/db_pool.rs`. A registry maps an exact `PathBuf` to a pool with at most four connections. Checkout reuses an idle connection, opens a configured connection while below the cap, or waits on a condition variable until one is returned. Dropping `PooledConnection` returns it to a live pool. Invalidation marks the removed pool closed, drops all idle connections, wakes waiters, and drops rather than recycles later returns; maintenance then waits for all checked-out leases to drain. Other commands and services call the same lease-owning `open_connection` boundary directly, including video-path resolution.

### Schema

`assets` is the central row for one indexed path.

| Column | Contract |
| --- | --- |
| `id` | `INTEGER PRIMARY KEY AUTOINCREMENT`; stable database identity used by mappings and IPC. |
| `path` | Required and globally unique. Source media remains outside app data. |
| `file_name` | Required for a newly created schema, default `''`; basename derived from `path`. |
| `kind` | Required media kind (`image`, `gif`, or `video` in normal indexed data). |
| `size_bytes`, `modified_at` | Required source metadata. `modified_at` is also the thumbnail-version value used by failure records and thumbnail preservation. |
| `width`, `height`, `duration_ms` | Optional extracted media metadata. |
| `thumb_path` | Optional path to a generated thumbnail. |
| `is_favorite` | Required integer boolean, default `0`. |
| `media_group_key`, `media_group_order` | Optional display key and optional numeric order within a group. |
| `fingerprint_mtime_ns` | Required nanosecond-resolution scan fingerprint, default `0`. |
| `tag_count` | Required denormalized count, default `0`. |
| `file_name_key` | Required lowercase filename lookup key, default `''`. |
| `media_group_key_normalized` | Optional lowercase, trimmed group lookup key. |
| `indexed_at` | Required Unix timestamp, defaulting to `unixepoch()`; refreshed by asset upsert. |
| `record_version` | Required monotonic file-record version, default `1`; increments when scan upsert changes source fingerprint fields and when an application rename changes path identity. Duplicate mutation inputs use it with the expected path as a compare-and-swap guard. |

The remaining tables are:

| Table | Columns and relationships |
| --- | --- |
| `tags` | `id INTEGER PRIMARY KEY AUTOINCREMENT`; `name TEXT NOT NULL UNIQUE COLLATE NOCASE`. Case-insensitive uniqueness prevents two tag rows that differ only by case. |
| `asset_tags` | Composite primary key `(asset_id, tag_id)`. Both columns are required foreign keys; deleting an asset or tag cascades to the mapping row. This is the many-to-many asset/tag relationship. |
| `scan_roots` | `path TEXT PRIMARY KEY`. It contains only the normalized root string; no creation timestamp is stored. |
| `asset_scan_roots` | Composite primary key `(asset_id, root_path)` plus required `last_seen_generation`. Both foreign keys cascade, so deletion of either the asset or root removes the mapping. One asset can remain owned by another overlapping root. |
| `thumbnail_failures` | One row per `asset_id`, with required `failure_count`, optional `last_error`, required `last_failed_at`, and required `asset_modified_at`. It deliberately has no declared foreign key; database helpers remove orphaned and source-version-stale rows. |
| `library_metadata` | Integer values keyed by text. Current reserved rows are `revision` (initially `1`) and `performance_schema_version` (initially `0`, migrated to `3`). It has no foreign-key relationships. SQLite `application_id` and `user_version` separately identify the application and complete schema generation; neither replaces these runtime/backfill rows. |
| `pending_file_operations` | Durable source-file recovery journal keyed by `(operation_id, asset_id)`, storing action, original/staging/final paths, and a `committed` flag set in the same transaction as asset mutation/revision. It deliberately survives asset deletion and is reconciled at startup. |

Deleting an `assets` row automatically removes `asset_tags` and `asset_scan_roots`. Deleting a `scan_roots` row automatically removes its root mappings. Tags are not deleted by cascade when their final asset mapping disappears; asset/tag mutation and asset-removal helpers explicitly call orphan-tag cleanup. Thumbnail failures likewise require explicit cleanup because they are not foreign-keyed.

### Indexes

In addition to primary-key and unique indexes created by SQLite, initialization creates all of the following:

| Index | Definition / purpose |
| --- | --- |
| `idx_assets_kind` | `assets(kind)` |
| `idx_assets_modified_at` | `assets(modified_at DESC)` |
| `idx_tags_name` | `tags(name)`; additional to the unique constraint-backed index. |
| `idx_asset_tags_tag` | `asset_tags(tag_id)` |
| `idx_asset_tags_asset` | `asset_tags(asset_id)` |
| `idx_thumbnail_failures_last_failed_at` | `thumbnail_failures(last_failed_at DESC)` |
| `idx_assets_file_name` | `assets(file_name COLLATE NOCASE)` |
| `idx_assets_gallery_kind_favorite` | `assets(kind, is_favorite, modified_at DESC, id DESC)` |
| `idx_assets_tag_count` | `assets(tag_count, modified_at DESC, id DESC)` |
| `idx_assets_group_normalized` | `assets(media_group_key_normalized, modified_at DESC, id DESC)` |
| `idx_assets_file_name_key` | `assets(file_name_key, modified_at DESC, id DESC)` |
| `idx_asset_tags_tag_asset` | `asset_tags(tag_id, asset_id)` |
| `idx_asset_scan_roots_generation` | `asset_scan_roots(root_path, last_seen_generation, asset_id)` |

### In-place initialization, migrations, and backfills

There is no separate migration runner. `init_schema` is idempotent in-place initialization and uses `PRAGMA application_id`/`user_version` as a compatibility gate, not as a step-by-step migration ledger:

1. It creates all current tables, base indexes, and the two metadata rows when absent.
2. It inspects `PRAGMA table_info(assets)` and independently adds legacy-missing `is_favorite`, `media_group_key`, `media_group_order`, `file_name`, `fingerprint_mtime_ns`, `tag_count`, `file_name_key`, `media_group_key_normalized`, and `record_version` columns. The legacy `file_name` alteration adds nullable `TEXT`, then a transaction fills null or blank values with the basename extracted from either `\` or `/` paths.
3. When `performance_schema_version < 1`, it backfills blank `file_name_key` values with `lower(file_name)`, fills normalized group keys with `lower(trim(media_group_key))` for nonblank groups, recomputes every `tag_count` from `asset_tags`, and then records version `1`.
4. It creates the query-performance indexes.
5. When the version read at the beginning is below `2`, it backfills `asset_scan_roots` for every stored root. Matching is the exact root path or a Windows-style `root\%` prefix, and inserted mappings use generation `0`; it then records version `2`.
6. Below version `3`, one transaction rebuilds every `file_name_key` with Rust Unicode lowercase and converts legacy delimiter-bearing tags into the same normalized tokens that CSV and search can represent. It repairs mappings/counts, removes orphan tags, conditionally bumps the library revision when visible data changed, and records version `3`.
7. It runs `PRAGMA optimize` and records the current application ID and schema version.

`performance_schema_version` gates these derived-data and mapping backfills only. It is not a complete historical schema version: presence checks, `CREATE ... IF NOT EXISTS`, and index creation handle structural compatibility independently. Bundle validation permits markerless legacy databases only when the known core tables, columns, declared types, metadata rows, integrity, foreign keys, and stored value types are valid; future or foreign markers are rejected before migration.

### Derived-field invariants

Application-mediated writes maintain these invariants:

- `file_name` is the final non-empty segment of `path`, accepting both slash styles. `file_name_key` uses Rust Unicode lowercase. Asset upsert, rename, mapped bundle import, duplicate lookup, and CSV import share that key; they do not compare full paths or file contents.

Root-prefix SQL escapes `%`, `_`, and the escape character before using `LIKE ... ESCAPE '^'`, so filesystem names are always treated literally. On Unix, discovery skips paths that cannot be represented as UTF-8 instead of storing a lossy path identity.
- `media_group_key` preserves the trimmed display spelling supplied by command callers. Blank command input becomes `NULL`. `media_group_key_normalized` is `NULL` for a missing/blank key and otherwise `lower(trim(media_group_key))`; single and bulk group setters update raw and normalized values together.
- Tags entering command/query boundaries are trimmed, Unicode-lowercased by Rust, emptied values removed, and de-duplicated while preserving first-seen order. A tag containing whitespace, comma, semicolon, or a control character is rejected. `tags.name` is also unique with `NOCASE` collation. Replacing tags normalizes defensively, validates asset existence, deletes old mappings only for a change, inserts/reuses tag rows, derives `assets.tag_count` from mappings, removes unreferenced tag rows, and commits command-level mutation plus conditional revision bump in one IMMEDIATE transaction with bounded whole-transaction busy retry.
- `tag_count` equals the number of normalized tags assigned through the tag helpers. The session query uses this field for exact-count filters; startup's version-1 backfill repairs it from mappings for pre-performance-schema databases.
- Scan upsert stores the exact nanosecond fingerprint separately from the seconds-based `modified_at`. An unchanged `modified_at` preserves an existing thumbnail path; a changed value takes the incoming thumbnail value. Thumbnail failures are considered current only when their `asset_modified_at` equals the asset's `modified_at`.

These are code invariants, not SQLite generated columns, checks, or triggers. Callers that issue ad hoc SQL or call low-level helpers with unnormalized values can violate them.

### Filter semantics

The primary gallery path normalizes filters at the command boundary and builds an ordered ID snapshot:

- All filter families combine with logical `AND`.
- `tags_and` means the asset must contain every requested tag. SQL groups mappings per asset and requires the number of distinct matched tag IDs to equal the number of normalized requested tags.
- `tags_not` means the asset must contain none of the requested tags; one matching excluded tag rejects it through `NOT EXISTS`.
- `favorites_only` requires `is_favorite = 1`; false adds no favorite predicate.
- A supported kind requires exact equality on `assets.kind`. The IPC boundary lowercases kind and discards unsupported values, which therefore act like no kind filter.
- `HasNoTags { tag_count }` is historically named but means exact tag count for every non-negative value, including zero. The session query compares `assets.tag_count` directly.
- `GroupName` is an exact, case-insensitive, outer-whitespace-insensitive group match through `media_group_key_normalized`; it is not a substring search. A blank group name and a negative tag count are rejected by the IPC boundary.

The registered legacy `list_assets` path applies the same user-visible composition and ordering, but computes exact tag counts from `asset_tags` and tests raw group text with `lower(trim(...))` instead of using the denormalized columns. Offset is floored at zero and limit is clamped to 1–500. Session pages clamp limit to 1–256 and clamp an offset beyond the end to the total.

### Gallery and media-group ordering

The legacy and session queries use the same bucket ordering:

1. Every nonblank `media_group_key` forms a group bucket. Every ungrouped asset forms its own singleton bucket.
2. Buckets sort by their newest member's `modified_at` descending, then their maximum asset ID descending.
3. Within a bucket, rows with a non-null `media_group_order` come before rows whose order is null.
4. Members then sort by `media_group_order` ascending, `modified_at` descending, and `id` descending.

This keeps the members returned by a query adjacent and positions the whole group by its newest matching member. Filtering happens before bucket statistics are calculated, so only members that pass the active filters affect the visible group's position. Summary materialization fetches IDs in chunks of 500 and reconstructs the requested ID order after SQLite returns the rows.

### Library revision and query sessions

`library_metadata.revision` starts at `1` and is incremented with one SQL update. It is the invalidation epoch for process-wide asset-query sessions, not a database migration version.

A query start is registered process-wide in arrival order before any blocking work is scheduled; the registration token plus the client generation decide supersession. It then reads the current revision inside one deferred read transaction that also builds the ordered ID list and materializes the first page, so revision, snapshot IDs, and first-page summaries share a single SQLite snapshot. The expensive ordered-ID build checks a cooperative cancellation flag periodically and returns `None` when the request has been superseded mid-build. The cache key includes the revision and normalized filters. A ready session holds the full ordered asset-ID vector plus that revision; later pages read their summaries inside an equivalent read snapshot after re-checking the session revision. The cache retains at most four sessions, uses least-recently-used promotion, and expires a session after five minutes without access. A later page returns `stale` and removes the session when its stored revision differs from the database; missing, expired, or evicted sessions are also `stale`. Bundle restore additionally clears the query manager and invalidates its connection pool before replacement.

The following mutation families bump the revision:

| Mutation family | Bump behavior |
| --- | --- |
| Replace one asset's tags | In the same transaction when canonical tags changed or persisted tag invariants were repaired; missing IDs reject. A fully canonical no-op does not bump. |
| Bulk tag merge | In the same transaction, only when at least one existing asset changed; missing IDs are skipped and reported through canonical results. |
| Set one favorite | In the same IMMEDIATE transaction as the update, including a no-op or missing ID. |
| Set one media group | In the same IMMEDIATE transaction as the update, including a no-op or missing ID. |
| Bulk media-group set | Only when at least one existing row changed, inside the same transaction as the updates. |
| Delete one asset | In the same transaction as the CAS deletion and orphan cleanup, after an existing source has moved to staging or absence has been confirmed. |
| Rename one asset | In the same transaction as the CAS path/version update, after staged filesystem installation. |
| Resolve duplicate batch | Exactly once in the transaction containing every CAS rename/delete and cleanup. A pre-commit rollback does not bump. |
| Remove a scan root | In the same transaction as root/orphan removal, only when orphaned assets were removed. |
| Non-empty scan/rescan | Once per committed indexing batch (inside the batch transaction) and once more after all processed roots; an empty root list returns without a bump. |
| CSV import | Once for the whole file, inside the single transaction that applies every row; the bump commits atomically with the applied rows. |
| Clear library | Always after the database deletion sequence. |
| Bundle restore | After installing and initializing the restored database; cached query state is also explicitly cleared. |

Thumbnail-path and thumbnail-failure writes, adding a scan root without scanning, and other metadata-neutral reads/maintenance do not bump the revision because they do not change filter membership or gallery order. Query-visible changes made directly through low-level `db.rs` helpers also do not bump automatically; the orchestrating command/service owns that responsibility.

### Transaction boundaries

The following multi-statement database operations use a SQLite transaction: legacy filename backfill; staged single/batch file mutation with one revision bump; legacy low-level rename; bulk group updates; tag replacement; bulk tag merge; scan-root removal plus orphan pruning; batches of scan-root touches; completed-generation pruning; scan write batches of up to 512 assets; and batch thumbnail-path updates. Batch renames first move DB paths to operation-private temporary identities so SQLite uniqueness does not make ordered rename cycles implicit; filesystem source-target cycles are rejected before mutation to keep rollback deterministic.

Single SQL statements are atomic individually. Tag replacement, bulk tag merge, single favorite/group setters, bulk media-group updates, and CSV import apply their conditional revision bump in the same transaction as the mutation (IMMEDIATE with bounded busy retry where writers contend). CSV parsing and validation complete before that transaction starts. Scan indexing bumps inside each batch transaction so an interrupted scan still invalidates sessions for everything it committed. Filesystem deletion, rename, thumbnail cleanup, bundle movement, and CSV parsing are outside SQLite transactions.

## Known limitations

- Schema initialization is not wrapped in one encompassing transaction. A failure can leave some tables, columns, indexes, or backfills applied while `performance_schema_version` still has its earlier value. Re-running initialization is intended to continue, but the version rows do not prove that every structural statement committed as one unit.
- The version-1 and version-2 backfills each perform multiple autocommit statements before updating their marker. The version read once near the start is reused for both decisions. There is no downgrade path, checksum, migration history, or rejection of a database with a future version.
- Derived columns are maintained only by application code. There are no triggers or constraints checking `tag_count`, `file_name_key`, `media_group_key_normalized`, boolean range, non-negative sizes/counts, valid media kinds, finite group order, or path/root normalization. A database imported with `performance_schema_version >= 2` is not given a general consistency rebuild.
- `thumbnail_failures.asset_id` has no foreign key. Most deletion paths clean failures explicitly and read paths purge stale rows, but referential integrity is eventual and helper-dependent.
- Media-group ordering buckets use the case-preserving raw `media_group_key`, while group filtering uses the normalized key. Consequently, differently cased stored keys can match one group filter but form separate ordering buckets.
- A session snapshots IDs and order, not complete asset rows. If query-visible code changes a row without a successful revision bump, later pages can materialize changed summaries against the old ID order. Favorite and media-group single-item setters deliberately bump for no-ops and missing IDs, causing harmless extra invalidation; tag replacement is no-op-aware and rejects missing IDs.
- The query-start snapshot covers revision, ordered IDs, and the first page, but the read transaction is deferred: a write that commits between registration and the first read can still be included, which is safe. A mutation committing after the snapshot becomes visible only through the later `stale` page transition.
- `clear_library_data` performs its asset, failure, tag, and root deletes as separate autocommit statements. Delete-by-prefix also combines explicit cleanup statements without a transaction. Safe single-asset and duplicate-batch file mutations use their dedicated transaction instead.
- Database transactions cannot make filesystem workflows atomic. Safe file mutations stage on each source filesystem and use a durable journal plus rollback/recovery outcomes; final staged-delete and thumbnail cleanup remain post-commit work. Bundle restore separately uses its own staging and rollback attempts.
- The query pool bounds checkout at five seconds per acquisition attempt and reports a busy failure when exhausted; a caller that repeatedly retries can still wait indefinitely in aggregate. Maintenance invalidation wakes old-pool waiters and waits for checked-out connections to return, so a stalled database caller can correspondingly stall maintenance without a timeout.

## Safe schema-change checklist

1. Update the current `CREATE TABLE IF NOT EXISTS` definition and add an idempotent compatibility path for existing databases. Do not assume changing the create statement migrates an installed database.
2. Decide whether the change is structural, a derived-data/performance backfill, or both. Add explicit version gating for one-time data work; do not reuse an existing `performance_schema_version` value with new meaning.
3. Preserve `foreign_keys = ON` on every connection. For a new relationship, specify nullability, uniqueness, cascade/restrict behavior, cleanup of existing orphans, and indexes for both join directions actually queried.
4. For a denormalized field, update every write path (single, bulk, scan, rename, CSV import, restore/migration) and provide a deterministic backfill. Test the stored value against the normalized source-of-truth relationship.
5. Re-evaluate AND/NOT/count/group semantics and the complete ordering tuple. Query changes must be kept equivalent between session IDs and the registered legacy query unless an intentional IPC migration says otherwise.
6. Decide whether each new mutation changes filter membership, group adjacency, or order. Put the mutation and revision increment in one transaction where possible; otherwise document the failure window. Update the [IPC contract](../architecture/ipc-contract.md) when session invalidation behavior changes.
7. Consider WAL state and open pooled connections for backup/restore or database replacement changes. Checkpoint intentionally, invalidate query state at the correct point, and retain rollback behavior described in [data safety and portability](data-safety-and-portability.md).
8. Exercise migration from the oldest supported schema and repeat initialization to prove idempotence. Inspect `PRAGMA table_info`, `PRAGMA index_list`, metadata versions, foreign-key behavior, and derived-field values rather than testing only a fresh in-memory database.
9. Add focused `db.rs` unit tests for SQL semantics; file-backed integration tests for WAL, migration, constraints, and transaction rollback; query-manager tests for revision/cache behavior; and backend workflow tests when commands or services own the transaction boundary.
10. Run formatting/checking plus the relevant Rust unit, integration, and backend workflow suites, then update this page and any affected architecture/IPC/subsystem references in the same change.

### Relevant existing tests

- `src-tauri/src/db.rs` unit tests cover root-prefix safety, AND/NOT/kind and favorite filters, exact tag counts, exact case-insensitive group filtering, pagination and grouped ordering, legacy-column addition, filename/thumbnail upsert behavior, tag search and merge, group mutations, failure-version cleanup, deletion, duplicate filename lookup, and rename persistence.
- `src-tauri/src/utils/tags.rs` tests lock down trimming, lowercasing, de-duplication, stable merge order, and CSV tag splitting.
- `src-tauri/tests/backend_integration.rs` uses a file-backed database for asset/tag/query flow, distinct thumbnail collection, library clearing, transactional bulk tag merge, and bulk group replacement.
- `src-tauri/tests/backend_e2e.rs` covers the database-level CSV merge/group/library-clear workflow across multiple helpers.
- Command/service tests in `commands/scan.rs`, `services/scan_service.rs`, `services/csv_service.rs`, and `services/backup_service.rs` cover root persistence/removal, safe scan cleanup, CSV parsing and atomic publication, bundle sidecar inclusion, restore validation, and restored database/thumbnail contents.

These tests validate many data primitives, but they do not substitute for the missing migration matrix or deliberate rollback/fault-injection tests listed under Known limitations. `services/asset_query_service.rs` now has a focused unit-test module covering equal-key session reuse, registration-order and generation supersession, revision-stale pages, LRU/TTL eviction, clear semantics, and snapshot start results; `db.rs` covers cooperative ID-build cancellation plus favorite/group/bulk-group same-transaction revision bumps. The connection pool's busy timeout itself is not yet directly unit-tested.
