# IPC contract

This document describes the current boundary between the React frontend and the Tauri backend. The registration list in `src-tauri/src/lib.rs`, the Rust command signatures and models, and the wrappers in `src/api.ts` are the source of truth. A function that exists in Rust but is not registered is not part of this contract.

For bootstrap, managed state, lock ordering, and module ownership, see the [system overview](system-overview.md). Domain behavior belongs in the canonical subsystem documentation for [database](../subsystems/database.md), [scanning and indexing](../subsystems/scanning-and-indexing.md), [library query and gallery](../subsystems/library-query-and-gallery.md), [thumbnails](../subsystems/thumbnails.md), [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md), [lightbox](../subsystems/lightbox.md), [settings operations](../subsystems/settings-operations.md), and [data safety and portability](../subsystems/data-safety-and-portability.md). This page records only what crosses IPC and the boundary behavior callers may rely on.

## Current guarantees

### Invocation and serialization rules

- Frontend code calls the typed functions in `src/api.ts`; those functions call the snake-case command names registered by `tauri::generate_handler!`.
- Invoke argument keys are camelCase (`assetId`, `tagsAnd`, `pageSize`, `onEvent`), while the corresponding Rust parameters are snake_case (`asset_id`, `tags_and`, `page_size`, `on_event`). Nested inputs opt into or explicitly define camelCase where needed: `BulkMediaGroupUpdateInput` uses `rename_all = "camelCase"`, and `AssetMetaFilterInput` uses `type`, `hasNoTags`/`groupName`, `tagCount`, and `groupName`.
- Ordinary Rust response structs serialize their fields exactly as declared, so response object fields are snake_case. This matches the interfaces in `src/types.ts`, including `session_id`, `thumb_path`, `removed_assets`, and similar fields.
- Tagged enums are the exceptions for discriminants. Query results use `{ status: "ready" | "stale" | "superseded", ... }`; thumbnail channel messages use `{ event: "ready" | "failed" | "done", data: ... }`; scan completion values are `"complete"` and `"partial"`. The data fields inside those variants remain snake_case.
- Rust `Option<T>` becomes `T | null` over IPC. The wrappers explicitly send `null` for media kind `"all"`, an absent search meta-filter, and nullable media-group values. `get_asset_details` and `ensure_asset_thumbnail` also use `null` for not found/unavailable results. Rust `()` is exposed as `Promise<void>`.
- Rust `i64`, `u64`, and `usize` values are represented as JavaScript `number`. Current IDs, counts, revisions, offsets, and session IDs are expected to remain within JavaScript's safe-integer range; the type layer does not enforce that bound.
- `src/types.ts` mirrors the serialized Rust models used by the wrappers. `AssetDetails` is flattened on the Rust side and therefore correctly extends `AssetSummary` in TypeScript. `AssetQueryFilters` is a frontend description rather than a returned transport model. `ScanSummary.completion` is currently optional in TypeScript for compatibility, although Rust always emits it.
- All registered command failures reject the invoke promise with text. Command boundaries return `Result<_, String>` and flatten `AppError`, worker-join failures, validation failures, and platform errors to a message; there is no structured IPC error code or error payload.

### Shared response shapes

The Rust models in `src-tauri/src/models.rs` produce these wire shapes and the TypeScript declarations in `src/types.ts` accept them. A `?` suffix below means that the field value is nullable, not that the field is omitted.

| Rust / TypeScript model | Serialized fields |
| --- | --- |
| `Asset` / `Asset` | `id`, `path`, `kind`, `size_bytes`, `modified_at`, `width?`, `height?`, `duration_ms?`, `thumb_path?`, `is_favorite`, `media_group_key?`, `media_group_order?`, `tags` |
| `AssetSummary` / `AssetSummary` | `id`, `file_name`, `preview_path?`, `kind`, `modified_at`, `width?`, `height?`, `duration_ms?`, `thumb_path?`, `is_favorite`, `media_group_key?`, `media_group_order?` |
| `AssetDetails` / `AssetDetails` | All `AssetSummary` fields flattened into the object, plus `path`, `size_bytes`, `tags` |
| `AssetPage` / `AssetPage` | `items: Asset[]`, `total` |
| `TagListPage` / `TagListPage` | `items: string[]`, `total` |
| `StartAssetQueryResult` / `StartAssetQueryResult` | `ready` has `session_id`, `revision`, `total`, `offset`, `items: AssetSummary[]`; `superseded` has only `status` |
| `AssetQueryPageResult` / `AssetQueryPageResult` | `ready` has the same fields as query-start `ready`; `stale` has only `status` |
| `ScanSummary` / `ScanSummary` | `completion: "complete" \| "partial"`, `indexed`, `removed`, `failed` |
| `RemoveRootSummary` / `RemoveRootSummary` | `removed_assets`, `removed_thumbnails` |
| `DeleteAssetSummary` / `DeleteAssetSummary` | `removed_assets`, `removed_thumbnails`, `source_status: "deleted" \| "missing" \| "cleanup_pending"`, `revision`, `recovery_path?` |
| `DuplicateAsset`, `DuplicateGroup`, `DuplicateScanSummary` / same | Asset: `id`, `path`, `record_version`, `size_bytes`, `fingerprint_mtime_ns`; group: `file_name`, `assets`; summary: `groups`, `duplicate_groups`, `duplicate_assets`, `revision` |
| `RenameAssetSummary` / `RenameAssetSummary` | `asset_id`, `old_path`, `new_path`, `removed_thumbnails`, `revision`, `status: "renamed" \| "cleanup_pending"`, `recovery_path?` |
| `DuplicateResolutionBatchSummary` / same | `status: "committed" \| "rolled_back" \| "recovery_required"`, `revision`, `results`, `removed_thumbnails`; each result has `asset_id`, item `status`, `old_path`, `new_path?`, and `recovery_path?` |
| `SetAssetTagsSummary` / `SetAssetTagsSummary` | `asset_id`, `changed`, `tags`, `revision` |
| `BulkTagMergeSummary` / `BulkTagMergeSummary` | `processed_assets`, `updated_assets`, `processed_asset_ids`, `updated_asset_ids`, `results`, `revision` |
| `BulkMediaGroupSummary` / `BulkMediaGroupSummary` | `processed_assets`, `updated_assets`, `media_group_key?` |
| `ThumbnailRenderSummary` / `ThumbnailRenderSummary` | `generated`, `failed`, `skipped_failed`, `processed`, `total`, `cancelled` |
| `ThumbnailBatchItem`, `ThumbnailBatchResult` / same | Item: `asset_id`, `thumb_path`; result: `ready: ThumbnailBatchItem[]`, `failed: number[]` |
| `ClearLibrarySummary` / `ClearLibrarySummary` | `removed_assets`, `removed_roots`, `removed_thumbnails` |
| `CsvExportSummary`, `CsvImportSummary` / same | Export: `rows`; import: `rows_read`, `rows_applied`, `assets_matched`, `assets_updated` |
| `DbBundleExportSummary`, `DbBundleImportSummary` / same | Export: `copied_files`, `copied_thumbnails`; import: `restored_files`, `restored_thumbnails` |
| `DbBundleInspection` / same | `format_version?`, `source_platform?`, `source_thumbs_dir?`, `roots`, `requires_mapping` |
| `VideoToolStatus` / same | `ffmpeg_available`, `ffprobe_available` |
| `ScanProgress` / `ScanProgress` | `phase`, `processed`, `total`, `message` |

`AssetSummary.preview_path` contains the full source path for GIF and video rows and is null for ordinary image rows. This lets the lightbox use a valid immediate media source while details continue loading.

`kind` is typed as `MediaKind` (`image | gif | video`) in TypeScript, while the serialized Rust model stores it as an unrestricted `String`; correctness currently comes from indexing/database invariants rather than serde validation on output.

### Asset queries and reads

The session API is the primary gallery query path. `listAssets`/`list_assets` remains a registered legacy page API and is still used by some tests as a mock implementation aid, but production gallery code uses `startAssetQuery` followed by `getAssetQueryPage`.

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `startAssetQuery` / `start_asset_query` | `tagsAnd: string[]`, `tagsNot: string[]`, `kind: MediaKind \| null`, `favoritesOnly: boolean`, `metaFilter: SearchMetaFilter \| null`, `generation: number`, `pageSize: number` (default 128) | `StartAssetQueryResult` (`ready` or `superseded`) | Normalizes filters, builds or reuses a revision-bound ID session, and includes the first page in a `ready` result. Backend page size is clamped to 1–256. The `generation` compatibility field is accepted but ignored by Rust. |
| `getAssetQueryPage` / `get_asset_query_page` | `sessionId: number`, `offset: number`, `limit: number` (default 128) | `AssetQueryPageResult` (`ready` or `stale`) | Returns summaries from the frozen session order. Limit is clamped to 1–256 and offset past the end is clamped to the total. |
| `getAssetDetails` / `get_asset_details` | `assetId: number` | `AssetDetails \| null` | Returns the full path, size, and tags in addition to summary fields; unknown IDs return `null`. |
| `getVideoStreamUrl` / `get_video_stream_url` | `assetId: number` | `string` | Validates that the ID identifies an existing video file and returns a process-private loopback HTTP URL. The endpoint resolves the path from SQLite again and never accepts a frontend-supplied filesystem path. |
| `listAssets` / `list_assets` | `offset`, `limit`, `tagsAnd`, `tagsNot`, `kind`, `favoritesOnly`, `metaFilter` | `AssetPage` | Legacy direct query returning full `Asset` rows. Negative offsets become 0 and limits are clamped to 1–500. It does not provide session consistency. |
| `listTags` / `list_tags` | `query: string`, `offset: number`, `limit: number` | `TagListPage` | Trims the search query, matches tag names case-insensitively, clamps offset to at least 0, and clamps limit to 1–200. |

`SearchMetaFilter` is a discriminated input union:

- `{ type: "hasNoTags", tagCount: number }` requires a non-negative count.
- `{ type: "groupName", groupName: string }` trims the group name and rejects an empty result.

For both session and legacy queries, tags are trimmed, lowercased, de-duplicated in first-seen order, and empty values are removed. Media kind is trimmed and lowercased; only `image`, `gif`, and `video` survive, so an unknown kind currently behaves like no kind filter rather than producing an error.

### Asset mutations and duplicate operations

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `setAssetTags` / `set_asset_tags` | `assetId`, `tags: string[]` | `SetAssetTagsSummary` | Atomically replaces normalized tags and conditionally bumps revision. Missing IDs reject. A canonical no-op returns `changed: false`; repairing legacy tag spelling, duplicate normalized tag rows, or a stale `tag_count` returns `changed: true` and bumps revision. |
| `mergeAssetTagsBulk` / `merge_asset_tags_bulk` | `assetIds: number[]`, `tags: string[]` | `BulkTagMergeSummary` | Atomically merges normalized tags and revision, skips missing IDs, and returns canonical tags/results for processed IDs. |
| `setAssetFavorite` / `set_asset_favorite` | `assetId`, `isFavorite: boolean` | `void` | Sets the favorite flag and bumps the library revision. |
| `setAssetMediaGroup` / `set_asset_media_group` | `assetId`, `mediaGroupKey: string \| null`, `mediaGroupOrder: number \| null` | `void` | Trims the key and maps blank to `null`; non-finite order is discarded as `null`; bumps the revision. |
| `setAssetsMediaGroupBulk` / `set_assets_media_group_bulk` | `updates: { assetId, mediaGroupOrder }[]`, `mediaGroupKey: string \| null` | `BulkMediaGroupSummary` | Drops non-positive/duplicate IDs, trims blank key to `null`, maps non-finite orders to `null`, and bumps the revision only when rows changed. The returned key is the normalized value. |
| `deleteAsset` / `delete_asset` | `assetId` | `DeleteAssetSummary` | Rejects an unknown ID. An existing source is moved to same-directory staging before one DB/revision transaction; a DB failure restores it. A missing source commits stale metadata removal with `source_status: "missing"`; failed final staged cleanup returns `cleanup_pending` and a recovery path. |
| `findDuplicateAssets` / `find_duplicate_assets` | none | `DuplicateScanSummary` | Groups database assets by duplicate file name and emits duplicate-scan progress. It does not hash file contents. |
| `renameAssetFile` / `rename_asset_file` | `assetId`, `newFileName` | `RenameAssetSummary` | Uses the same staged service as the batch. Portable validation also rejects control characters, trailing dot/space, and reserved Windows device names. Target installation is atomic no-clobber on supported Linux/Windows targets; DB mutation and revision are one transaction. |
| `applyDuplicateResolutionBatch` / `apply_duplicate_resolution_batch` | `input: { scanRevision, changes }`; each tagged change includes `assetId`, `expectedPath`, `expectedRecordVersion`, and rename also has `newFileName` | `DuplicateResolutionBatchSummary` | Validates the complete snapshot, IDs, versions, names, filesystem and DB collisions, and touched duplicate groups before mutation. Runs once under combined locks, stages all sources, commits one DB transaction/revision, and returns per-item commit, rollback, or recovery state. Rename targets that are another batch source are rejected. |

The single-item favorite and group setters do not validate that an ID affected a row before resolving. Single tag replacement rejects a missing ID; bulk tag merge skips missing IDs and reports processed IDs. See [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md) and [lightbox](../subsystems/lightbox.md) for the user workflows built on these calls.

### Scan roots and scanning

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `scanFolder` / `scan_folder` | `path: string` | `ScanSummary` | Normalizes and validates an existing directory, persists it as a root, then scans it. Runs blocking work off the async runtime and emits scan progress. |
| `listScanRoots` / `list_scan_roots` | none | `string[]` | Returns stored roots sorted by newest creation first. |
| `addScanRoot` / `add_scan_root` | `path: string` | `void` | Normalizes and validates an existing directory before persisting it; it does not scan. |
| `removeScanRoot` / `remove_scan_root` | `path: string` | `RemoveRootSummary` | Normalizes the path, removes the root and assets no longer covered by another root, bumps the revision, and best-effort deletes their thumbnails. |
| `rescanAllRoots` / `rescan_all_roots` | none | `ScanSummary` | Scans all stored roots; no roots yields a complete, zero-count summary and a `scan-empty` event. Runs blocking work off the async runtime. |

Root normalization trims whitespace, converts `/` to `\`, and removes trailing separators except for a drive root. `ScanSummary` always contains `completion`, `indexed`, `removed`, and `failed`; partial discovery/indexing failures produce `completion: "partial"` and preserve stale rows for the incomplete root.

### Thumbnails

`ensureThumbnailsStream`/`ensure_thumbnails` is the primary on-demand gallery API. `ensurePageThumbnails` and its `thumbnail-ready` broadcast are retained legacy behavior; `ensureAssetThumbnail` is the single-item compatibility API. The current production queue consumes only the channel API.

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `ensureThumbnailsStream` / `ensure_thumbnails` | `requestId`, `visibleIds`, `prefetchIds`, `onEvent: Channel<ThumbnailStreamEvent>` | `void` after processing | Filters IDs to positive unique values, takes at most 64 visible and then 64 additional prefetch IDs, prioritizes visible work, streams results, and persists paths/failures. The `requestId` compatibility field is accepted but ignored by Rust. |
| `ensurePageThumbnails` / `ensure_page_thumbnails` | `assetIds: number[]` | `ThumbnailBatchResult` | De-duplicates IDs, treats all as high priority, returns `ready` items and `failed` IDs, and also broadcasts `thumbnail-ready` for every ready item. No backend batch-size clamp is applied. |
| `ensureAssetThumbnail` / `ensure_asset_thumbnail` | `assetId` | `string \| null` | Reuses a valid thumbnail or generates one at high priority. Unknown assets, missing source files, and generation failures return `null`; failure state is persisted. |
| `renderAllThumbnails` / `render_all_thumbnails` | none | `ThumbnailRenderSummary` | Starts one process-wide bulk run; a concurrent bulk run rejects. Previously recorded failures are counted as `skipped_failed`. |
| `renderFailedThumbnails` / `render_failed_thumbnails` | none | `ThumbnailRenderSummary` | Same bulk-run guard, but retries only recorded failures and does not skip them. |
| `cancelRenderAllThumbnails` / `cancel_render_all_thumbnails` | none | `boolean` | Returns `true` and sets the cancellation flag only while either bulk mode is running; already completed ready results are retained. |
| `clearAllThumbnails` / `clear_all_thumbnails` | none | `number` | Clears database thumbnail paths, exclusively blocks thumbnail readers, best-effort deletes files, emits progress, and returns the number of files actually removed. |

The channel is created in `src/api.ts`; `onEvent` is not JSON data. Its serialized messages are:

```ts
type ThumbnailStreamEvent =
  | { event: "ready"; data: { asset_id: number; thumb_path: string } }
  | { event: "failed"; data: { asset_id: number } }
  | { event: "done"; data: { ready: number; failed: number } };
```

`ready` is sent as each existing or newly generated thumbnail becomes available. After processing, failures are sent and one `done` summary is sent. Channel-send failures are deliberately ignored, so successful command completion does not prove that the receiver observed every message. The frontend queue uses its own generation counter to ignore late messages after reset; it does not rely on the ignored backend `requestId`.

The legacy `thumbnail-ready` application event carries one `ThumbnailBatchItem` payload (`asset_id`, `thumb_path`) and is emitted only by `ensure_page_thumbnails`. It is separate from the channel and from `process-progress`.

### Import, export, and destructive data operations

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `exportTagsCsv` / `export_tags_csv` | `path: string` | `CsvExportSummary` | Trims and rejects an empty target, creates parent directories, and writes `file_name,tags,favorite,media_group_key,media_group_order`. |
| `importTagsCsv` / `import_tags_csv` | `path: string` | `CsvImportSummary` | Requires an existing file, matches assets case-insensitively by file name, merges normalized tags, optionally updates favorite/group columns, and bumps the revision if any asset changed. Missing standard headers fall back to columns 0/1 for file name/tags; invalid optional values are ignored. |
| `exportDbBundle` / `export_db_bundle` | `path: string` | `DbBundleExportSummary` | Trims and rejects an empty target and creates a ZIP containing `media.db`, present WAL/SHM files, and thumbnail files. |
| `inspectDbBundle` / `inspect_db_bundle` | `path: string` | `DbBundleInspection` | Read-only inspection of a versioned manifest or legacy staged database; reports roots requiring Windows-to-Linux mapping. |
| `importDbBundle` / `import_db_bundle` | `path: string`, `rootMappings: { sourceRoot, targetRoot }[]` | `DbBundleImportSummary` | Validates and rewrites a staged database before replacement. Linux rejects Windows roots without complete mappings and detects mapped path collisions. |
| `clearLibraryData` / `clear_library_data` | none | `ClearLibrarySummary` | Removes assets and roots, bumps the revision, best-effort deletes thumbnail files, and emits library-clear progress. |

These operations' locking, transaction, archive, rollback, and filesystem guarantees are documented in [data safety and portability](../subsystems/data-safety-and-portability.md).

### Native window

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `syncNativeWindowTheme` / `sync_window_theme` | `theme: "light" \| "dark"` | `void` | Rejects any other string. Sets the native theme and background; on Windows it also sets caption, border, and title-text colors. |

### Query session states

A successful `start_asset_query` returns:

```ts
{
  status: "ready";
  session_id: number;
  revision: number;
  total: number;
  offset: 0;
  items: AssetSummary[];
}
```

Sessions hold an ordered asset-ID snapshot for one normalized filter key and library revision. The process-wide cache keeps at most four sessions, evicts least-recently-used entries, and expires entries after five minutes without access. An equal filter/revision key may reuse an existing session.

- `superseded` is a successful start result, not an invoke error. A non-cached query returns it if another start request became the process-wide latest request before its ID-list build completed. The supplied frontend `generation` does not participate in this decision. `useLibraryAssets` also compares its local generation and ignores an obsolete response.
- `stale` is a successful page result, not an invoke error. It means the session ID is missing, expired, evicted, or bound to an older library revision. A revision mismatch also removes that session. `useLibraryAssets` responds by starting a fresh query.
- `ready` pages preserve the session order and report the effective (possibly end-clamped) offset. Query-visible mutations and scans bump the library revision, so later pages from older sessions become stale.

Because supersession and the cache are process-wide, requests from another window or independent caller can supersede or evict this window's work.

### Broadcast progress

Longer operations broadcast the application event `process-progress` with this snake-case payload:

```ts
interface ScanProgress {
  phase: string;
  processed: number;
  total: number;
  message: string;
}
```

Current phase families are:

- scanning: `counting`, `scanning`, `cleanup`, `scan-empty`, `scan-skip`;
- bulk thumbnails: `thumbs-start`, `thumbs`, `thumbs-warn`, `thumbs-done`, `thumbs-failed-start`, `thumbs-failed`, `thumbs-failed-done`;
- on-demand/clear thumbnails: `thumbs-page-start`, `thumbs-page`, `thumbs-page-done`, `thumbs-clear`;
- library clear: `library-clear`, `library-clear-done`; and
- duplicates: `duplicates-scan`, `duplicates-scan-done`.

Consumers must filter by phase because the event name is shared and broadcasts are not correlated to a command invocation. `useSettingsOperationRunner` subscribes only for the duration of an operation and applies a phase matcher. Messages are display text, not stable machine-readable error data. Most emit sites intentionally discard delivery errors; the command result is authoritative for operation completion.

### Boundary validation and normalization summary

| Input | Boundary behavior |
| --- | --- |
| Session page size/limit | Clamp to 1–256; first-page wrapper default is 128. |
| Legacy asset page | Offset at least 0; limit 1–500. |
| Tag page | Offset at least 0; limit 1–200; query trimmed and case-insensitive. |
| Query/tag arrays | Trim, lowercase, remove empty strings, de-duplicate preserving first occurrence. |
| Query media kind | Trim/lowercase; unsupported values become no filter. |
| Meta-filter | Reject negative `tagCount` and blank `groupName`; require the tagged camelCase shape. |
| Bulk asset IDs | Tag/group mutation APIs drop non-positive and duplicate IDs. Streamed thumbnails do the same and cap visible/prefetch groups at 64 each. |
| Media group | Blank key becomes `null`; non-finite order becomes `null`. |
| Root path | Trim, use Windows separators, remove non-drive trailing separators; add/scan requires an existing directory. |
| CSV/bundle paths | Trim; exports reject empty paths and create parents; imports require an existing file. Bundle import also rejects unsafe ZIP paths and requires `media.db`. |
| Rename | File name only; rejects blank/dot names, separators, control/listed Windows-invalid characters, trailing dot/space, reserved device names, same path, and occupied filesystem or indexed destinations. |
| Theme | Exactly `light` or `dark`. |

Deserialization itself rejects missing required arguments, wrong JSON types, invalid tagged-union shapes, negative values sent to unsigned Rust parameters, and non-representable numbers before command logic runs. Frontend TypeScript types help normal callers but are not runtime validation for arbitrary IPC callers.

## Known limitations

- Rust and TypeScript payload types are maintained manually; there is no generated schema or compile-time cross-language parity check. `ScanSummary.completion` is looser in TypeScript than the current Rust response.
- `generation` on `start_asset_query` and `requestId` on `ensure_thumbnails` are ignored compatibility fields. Query supersession is process-global, while thumbnail staleness is handled only by frontend generation checks.
- `list_assets`, `ensure_asset_thumbnail`, and `ensure_page_thumbnails` remain registered alongside their primary session/channel replacements. The legacy thumbnail broadcast has no correlation identifier.
- IPC errors are text only. Consumers cannot reliably distinguish validation, not-found, busy, filesystem, database, worker, or platform failures except by message text.
- Numeric Rust IDs/counters are exposed as JavaScript `number` without an explicit safe-integer guard.
- Event delivery is best effort. `process-progress`, `thumbnail-ready`, and channel sends commonly ignore delivery errors; events can be missed and must not be used as a commit signal.
- `process-progress` is a shared broadcast with free-form `phase` and human-readable `message`; it has no request ID, sequence number, or version.
- Favorite and media-group setters resolve even if an unknown asset ID changed zero rows. Tag replacement, delete, and rename reject unknown IDs; bulk tag/group operations skip missing IDs and report processed counts or IDs.
- Runtime validation is uneven: unsupported query kinds degrade to no filter, non-finite group order degrades to `null`, and the legacy page-thumbnail endpoint has no batch cap.
- `src/__tests__/api.test.ts` covers legacy `listAssets`, tag-mutation payload/result seams, and media-path normalization, but it does not exhaustively lock down every wrapper, response shape, channel, or event.

## Safe contract-change checklist

When changing IPC:

1. Update the Rust command signature/model and the matching `src/api.ts` wrapper and `src/types.ts` type in the same change. Confirm command argument casing, nested serde casing, optional/null behavior, enum tags, and all numeric types.
2. If adding or removing a command, update the `tauri::generate_handler!` registration in `src-tauri/src/lib.rs`. Search for direct `invoke` calls so the wrapper remains the single frontend seam.
3. Preserve existing command names and payload fields when compatibility is required. For a migration, add the replacement first, move consumers and tests, then explicitly remove the legacy command/event in a later breaking change. Do not silently repurpose ignored compatibility fields.
4. For query-visible writes, decide when to bump the library revision and test the resulting `ready`/`stale` behavior. Test cache reuse, expiry/eviction assumptions, limit/offset clamps, concurrent starts, and frontend generation rejection if those semantics change.
5. For thumbnail streaming, test the exact tagged event shapes, positive-ID de-duplication, 64/64 caps, visible priority, one terminal `done`, failures, abandoned channels, and frontend reset behavior. Keep the channel distinct from broadcasts.
6. For progress, treat the event name, payload fields, and phase strings as contract. Update phase matchers and translations together, and never make successful event delivery a prerequisite unless the API is intentionally redesigned.
7. Keep command errors text-only unless Rust and every consumer migrate together to a versioned structured error payload. Do not branch new code on current message wording.
8. Re-evaluate boundary validation, workflow locks, revision/cache invalidation, filesystem rollback, and destructive-operation safety. Follow the [system overview](system-overview.md) and the relevant subsystem document rather than duplicating those policies here.
9. Add focused frontend wrapper tests and Rust serialization/command/service tests. Run the frontend, backend, and real Tauri E2E suites in proportion to the change; an IPC signature change requires a real desktop-path check, not only mocked `invoke` tests.
10. Update this document in the same change, including the primary/legacy designation and Known limitations.

### Relevant tests

- `src/__tests__/api.test.ts` checks invoke payload mapping for `list_assets` and slash normalization before `convertFileSrc`.
- `src/hooks/__tests__/useLibraryBrowser.test.ts` exercises the session-query wrapper contract through the library hooks, including filters and pagination.
- `src/hooks/__tests__/useThumbnailQueue.test.ts` exercises streamed ready/failed handling, batching, and ignoring messages after the frontend generation changes.
- `src/components/settings/services/__tests__/progressService.test.ts` locks down phase matching and progress-summary formatting; settings hook tests cover subscription lifetimes and command workflows.
- Rust tests in `src-tauri/src/commands/assets.rs`, `scan.rs`, `thumbs.rs`, and `import_export.rs` cover filename/root/CSV validation and cancellation behavior.
- Rust tests in `src-tauri/src/services/thumb_service.rs`, `scan_service.rs`, and `backup_service.rs`, plus `src-tauri/src/db.rs`, cover thumbnail results, partial scans, archive safety, query primitives, clamps, and persistence semantics. The query-session manager itself currently has no focused unit-test module.
- `src-tauri/tests/backend_integration.rs` and `backend_e2e.rs` cover file-backed cross-layer mutation/import workflows but do not exercise JavaScript serialization.
- `e2e/specs/*.e2e.js` exercises the registered commands through a real desktop WebView and is the strongest existing check for Rust/TypeScript integration.
