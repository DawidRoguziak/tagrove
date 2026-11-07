# 0009: Remove the legacy `list_assets` command

- Status: Accepted
- Date: 2026-11-07
- Areas: IPC, database, testing
- Related: [IPC contract](../architecture/ipc-contract.md#asset-queries-and-reads), [database](../subsystems/database.md), [testing](../development/testing.md), [plan stage 10](../plans/grid-performance/10-open-topics.md), [ADR 0004](0004-trigger-maintained-gallery-sort-keys.md)

## Context

`list_assets(offset, limit, …)` was the gallery's original query: one SQL statement with `COUNT(*)`, `OFFSET` and full `Asset` rows (path, size and tags for every row). Query sessions (`start_asset_query`, `get_asset_query_page`) replaced it in the gallery long ago. ADR 0004 left the command registered because it was still in use outside the app:

- 17 desktop E2E specs used it to read the library for assertions;
- about 30 Rust tests used `db::list_assets` / `list_assets_with_meta` as a helper, and seven of them were the only tests of filter semantics (kind, tag AND/NOT, favorites, exact tag-count meta filter, group name, group adjacency).

The application's own code no longer called it: the TypeScript wrapper `listAssets` was imported only by its payload test. The command was a second, separately maintained implementation of the query semantics (exact tag counts from `asset_tags`, raw group text) that the gallery never ran, and its filter tests did not cover the session path that the gallery does run.

## Options considered

1. **Keep the command as a test aid.** No work, but a public IPC surface that no user path needs, and a second query implementation with tests that prove nothing about the gallery. Rejected.
2. **Keep the SQL functions and remove only the command.** Tests would keep exercising a query the app never runs. Rejected.
3. **Remove the command, its SQL functions and models, and move every test to the session order (chosen).**

## Decision

- Removed: the `list_assets` command and its service (`src-tauri/src/commands/assets.rs`), its `generate_handler!` entry (`src-tauri/src/lib.rs`), `db::list_assets` / `list_assets_with_meta` and `parse_asset_row` (`src-tauri/src/db/queries.rs`), the `Asset` and `AssetPage` models, the `listAssets` wrapper, and the `LegacyAsset` / `AssetPage` contract types.
- Rust tests read a page of the session order through test-only helpers: `db::test_support::session_page[_with_meta]` for unit tests, `src-tauri/tests/support/assets.rs` for integration targets. Both cut `list_ordered_asset_ids_with_meta` (the session vector) to a page and read each row through `get_asset_details`. The seven filter tests now run on the session path (renamed `session_order_*` in `src-tauri/src/db.rs`).
- Desktop E2E specs use `listAssetsForAssertions` (`e2e/assets.js`): `start_asset_query` with its own owner (`e2e-assertions`) and a generation that grows across page reloads, pages through `get_asset_query_page`, then `get_asset_details` per row. Items are `AssetDetails` in gallery order.

## Consequences

- One query implementation: filter tests, E2E assertions and the gallery all use the session order. E2E lists are now in gallery order rather than the old `list_assets` order.
- IPC contract change (non-additive): calling `list_assets` now fails as an unknown command. Nothing in the application called it, so there is no data or profile migration.
- `start_asset_query` admission is global: a start from any owner supersedes another owner's start that is still running, and the app then shows its load error. `listAssetsForAssertions` waits until the app has no `start_asset_query` in flight (observed through the IPC fetch bridge, from the first helper call on a page), retries its own superseded or stale reads, and presses the gallery's Retry if it still caught the app mid-start.
- Fetching details per row costs one IPC per asset; the helper batches 32 at a time. Specs read at most a few thousand rows.
