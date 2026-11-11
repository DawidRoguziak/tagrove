# IPC contract

Implementation entry points: [registered commands](../../src-tauri/src/lib.rs), [frontend wrappers](../../src/platform/tauri), [Rust models](../../src-tauri/src/models.rs), [TypeScript payloads](../../src/platform/tauri/contracts), [video payloads](../../src/platform/tauri/contracts/video.ts).

This page owns frontend/backend command names, payloads, result states, channels, and events. Rust functions absent from the registration list are internal helpers, not callable IPC commands.

For bootstrap, managed state, lock ordering, and module ownership, see the [system overview](system-overview.md). Domain behavior belongs in the canonical subsystem documentation for [database](../subsystems/database.md), [scanning and indexing](../subsystems/scanning-and-indexing.md), [library query and gallery](../subsystems/library-query-and-gallery.md), [thumbnails](../subsystems/thumbnails.md), [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md), [lightbox](../subsystems/lightbox.md), [settings operations](../subsystems/settings-operations.md), and [data safety and portability](../subsystems/data-safety-and-portability.md). This page records only what crosses IPC and the boundary behavior callers may rely on.

## Invocation and serialization rules

- Frontend code calls typed functions in `src/platform/tauri`, split into assets, tags, scanning, thumbnails, maintenance, video, and window modules. The functions call the snake-case command names registered by `tauri::generate_handler!`. Dialogs, events, the system opener, and native window access also enter through this directory.
- Invoke argument keys are camelCase (`assetId`, `tagsAnd`, `pageSize`, `onEvent`), while the corresponding Rust parameters are snake_case (`asset_id`, `tags_and`, `page_size`, `on_event`). Nested inputs opt into or explicitly define camelCase where needed: `BulkMediaGroupUpdateInput` uses `rename_all = "camelCase"`, and `AssetMetaFilterInput` uses `type`, `hasNoTags`/`groupName`, `tagCount`, and `groupName`.
- Ordinary Rust response structs serialize their fields exactly as declared, so response object fields are snake_case. This matches the interfaces in `src/platform/tauri/contracts`, including `session_id`, `thumb_version`, `removed_assets`, and similar fields.
- Tagged enums are the exceptions for discriminants. Query results use `{ status: "ready" | "stale" | "superseded", ... }`; scan completion values are `"complete"` and `"partial"`. The data fields inside those variants remain snake_case. The one binary response, `get_asset_query_id_range`, has no JSON body; see [binary ID ranges](#binary-id-ranges).
- Native video events use a `type` discriminant and include `session_id`. Video controls use camelCase discriminants and fields because their Rust enum has both `rename_all = "camelCase"` and `rename_all_fields = "camelCase"`.
- Rust `Option<T>` becomes `T | null` over IPC. The wrappers explicitly send `null` for media kind `"all"`, an absent search meta-filter, and nullable media-group values. `get_asset_details` also uses `null` for a missing row. Rust `()` is exposed as `Promise<void>`.
- Rust `i64`, `u64`, and `usize` values are represented as JavaScript `number`. Current IDs, counts, revisions, offsets, and session IDs are expected to remain within JavaScript's safe-integer range; the type layer does not enforce that bound.
- `src/platform/tauri/contracts` mirrors the serialized Rust models used by the wrappers. `AssetDetails` is flattened on the Rust side and therefore correctly extends `AssetSummary` in TypeScript. `AssetQueryFilters` is a frontend description rather than a returned transport model. `ScanSummary.completion` is required because Rust always emits it.
- All registered command failures reject the invoke promise with text. Command boundaries return `Result<_, String>` and flatten `AppError`, worker-join failures, validation failures, and platform errors to a message; there is no structured IPC error code or error payload. Some messages use the backend display-text format (`i18n:<key>` or a `{"key","params"}` JSON string, [settings operations](../subsystems/settings-operations.md)), which consumers translate with `translateBackendText` (`src/i18n/backendText.ts`) for display only; `delete_asset` emits `i18n:common.libraryBusy`.

Consumers import payloads from their owning contract module directly. `SelectedAsset` is frontend view state in
`src/features/lightbox/types.ts`, outside the wire contracts.

## Frontend transport ownership

| Module under `src/platform/tauri` | Commands and payload types |
| --- | --- |
| `assets.ts` / `contracts/assets.ts` | Query sessions, summaries/details, favorite/group writes, deletion and media URLs. |
| `tags.ts` / `contracts/tags.ts` | Tag listing, startup snapshot, replacements/merges and query-impact results. |
| `scanning.ts` / `contracts/scanning.ts` | Roots, scans, startup preferences and progress shape. |
| `thumbnails.ts` / `contracts/thumbnails.ts` | `thumb://` URL builder, bulk render/retry/cancel and cleanup. |
| `maintenance.ts` / `contracts/maintenance.ts` | CSV/bundles, duplicates, video-tool checks and library clearing. |
| `video.ts` / `contracts/video.ts` | Open reservations, session controls, labels, bounds and event channel. |
| `events.ts`, `dialogs.ts`, `window.ts` | Tauri event binding, native pickers and window access/theme. |

Models pass plain arrays/objects to these adapters. Keep Solid owners, reactive containers
and editor drafts out of serialized payloads. TypeScript declares the transport but does
not validate arbitrary external callers; Rust boundary validation remains authoritative.

## Shared response shapes

The Rust models in `src-tauri/src/models.rs` produce these wire shapes and the TypeScript declarations in `src/platform/tauri/contracts` accept them. A `?` suffix below means that the field value is nullable, not that the field is omitted.

| Rust / TypeScript model | Serialized fields |
| --- | --- |
| `AssetSummary` / `AssetSummary` | `id`, `file_name`, `preview_path?`, `kind`, `modified_at`, `width?`, `height?`, `duration_ms?`, `thumb_version`, `is_favorite`, `media_group_key?`, `media_group_order?` |
| `AssetDetails` / `AssetDetails` | All `AssetSummary` fields flattened into the object, plus `path`, `size_bytes`, `thumb_path?`, `tags` |
| `TagListPage` / `TagListPage` | `items: string[]`, `total` |
| `StartAssetQueryResult` / `StartAssetQueryResult` | `ready` has `session_id`, `revision`, `total`, `offset`, `items: AssetSummary[]`; `superseded` has only `status` |
| `AssetQueryPageResult` / `AssetQueryPageResult` | `ready` has the same fields as query-start `ready`; `stale` has only `status` |
| `ScanRoot` / `ScanRoot` | `path: string`, `auto_scan_on_startup: boolean` |
| `ScanSummary` / `ScanSummary` | `completion: "complete" \| "partial"`, `indexed`, `removed`, `failed` |
| `RemoveRootSummary` / `RemoveRootSummary` | `removed_assets`, `removed_thumbnails` |
| `DeleteAssetSummary` / `DeleteAssetSummary` | `removed_assets`, `removed_thumbnails`, `source_status: "deleted" \| "missing" \| "cleanup_pending"`, `revision`, `recovery_path?` |
| `DuplicateAsset`, `DuplicateGroup`, `DuplicateScanSummary` / same | Asset: `id`, `path`, `record_version`, `size_bytes`, `thumb_version`; group: `file_name`, `assets`; summary: `groups`, `duplicate_groups`, `duplicate_assets`, `revision` |
| `DuplicateResolutionBatchSummary` / same | `status: "committed" \| "rolled_back" \| "recovery_required"`, `revision`, `results`, `removed_thumbnails`; each result has `asset_id`, item `status`, `old_path`, `new_path?`, and `recovery_path?` |
| `SetAssetTagsSummary` / `SetAssetTagsSummary` | `asset_id`, `changed`, `tags`, `query_impact`, `vocabulary_changed`, `revision` |
| `BulkTagMergeSummary` / `BulkTagMergeSummary` | `processed_assets`, `updated_assets`, `processed_asset_ids`, `updated_asset_ids`, `results`, `query_impact`, `vocabulary_changed`, `revision` |
| `BulkFavoriteSummary` / `BulkFavoriteSummary` | `processed_asset_ids`, `is_favorite`, `revision`; an empty result reports `is_favorite: false` and leaves the revision unchanged |
| `BulkMediaGroupSummary` / `BulkMediaGroupSummary` | `processed_asset_ids`, `processed_assets`, `updated_assets`, `media_group_key?` |
| `ThumbnailRenderSummary` / `ThumbnailRenderSummary` | `generated`, `failed`, `skipped_failed`, `processed`, `total`, `cancelled` |
| `ClearLibrarySummary` / `ClearLibrarySummary` | `removed_assets`, `removed_roots`, `removed_thumbnails` |
| `CsvExportSummary`, `CsvImportSummary` / same | Export: `rows`; import: `rows_read`, `rows_applied`, `assets_matched`, `assets_updated` |
| `DbBundleExportSummary`, `DbBundleImportSummary` / same | Export: `copied_files`, `copied_thumbnails`; import: `restored_files`, `restored_thumbnails` |
| `DbBundleInspection` / same | `format_version?`, `source_platform?`, `source_thumbs_dir?`, `roots`, `requires_mapping` |
| `VideoToolStatus` / same | `ffmpeg_available`, `ffprobe_available` |
| `ScanProgress` / `ScanProgress` | `phase`, `processed`, `total`, `message` |

`AssetSummary.preview_path` contains the full source path for GIF and video rows and is null for ordinary image rows. Gallery GIF tiles use static thumbnails, never this original-media path. The lightbox accepts verified GIF/video preview paths while complete details load. Ordinary images wait for their detail path. Video opens by asset ID, not by a frontend path.

`controlLabels` includes localized `play`, `pause`, `mute`, `unmute`, `volume`, `seek`, `playbackRate`, `fullscreen`, and `exitFullscreen` strings. Each label must contain 1–120 visible characters after trimming.

`MpvVideoEvent` is `{ session_id, type, ... }`. Variants are `pointerActivity`, `loading`, `metadata` (`duration`, `width`, `height`), `snapshot` (`state`), `fullscreen`, `error` (`message`), and `controlError` (`message`). Snapshot state carries `session_id`, `duration`, `current_time`, `paused`, `seeking`, `buffering`, `volume`, `muted`, `rate`, and `fullscreen`. The worker reads libmpv properties rather than inferring playing state from seek completion. Each playback session has its own event client; the old decoder is stopped before the next client is created. The frontend accepts events only for the returned session, retaining bounded early events while open is pending. `pointerActivity` reports throttled GTK motion. Native bounds callbacks are delivered only after successful placement on the GTK thread, so opening a sidebar can wait until the surface is clear.

`kind` is typed as `MediaKind` (`image | gif | video`) in TypeScript, while the serialized Rust model stores it as an unrestricted `String`; correctness currently comes from indexing/database invariants rather than serde validation on output.

## Asset queries and reads

The session API is the only library query path: `startAssetQuery` followed by `getAssetQueryPage`. The former `list_assets` OFFSET page API was removed ([ADR 0009](../adr/0009-remove-list-assets-command.md)); desktop E2E reads the library for assertions through sessions (`e2e/assets.js`).

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `startAssetQuery` / `start_asset_query` | `tagsAnd: string[]`, `tagsNot: string[]`, `kind: MediaKind \| null`, `favoritesOnly: boolean`, `metaFilter: SearchMetaFilter \| null`, `ownerId: string`, `generation: number`, `pageSize: number` (default 128), `anchorOffset: number \| null` (optional, default `null`) | `StartAssetQueryResult` (`ready` or `superseded`) | Normalizes filters, admits only increasing generations within each owner before blocking work, builds or reuses a revision-bound ID session inside one SQLite snapshot, and includes one page in a `ready` result: the aligned page containing `anchorOffset` (clamped to the last row), or the first page without an anchor. `offset` reports that page's start. The frontend passes its first visible index so a refresh deep in the list gets its viewport page in one IPC. Backend page size is clamped to 1–256. The owner ID is nonempty and at most 128 UTF-8 bytes. Each frontend query model uses a new UUID, including after remount. Accepted requests receive process-wide registration tokens; frontend `generation` also rejects late results locally. |
| `getAssetQueryPage` / `get_asset_query_page` | `sessionId: number`, `offset: number`, `limit: number` (default 128) | `AssetQueryPageResult` (`ready` or `stale`) | Returns summaries from the frozen session order. Limit is clamped to 1–256 and offset past the end is clamped to the total. |
| `getAssetQueryIdRange` / `get_asset_query_id_range` | `sessionId: number`, `start: number`, `end: number` (inclusive) | Binary body, decoded to `AssetQueryIdRangeResult` (`{ status: "ready", ids: Float64Array }` or `{ status: "stale" }`) | Copies the ordered IDs `start..=end` from the session vector without SQL; `end` is clamped to the last row and `start > end` or `start` past the end yields a ready empty range. Same staleness check as page reads. Format under [binary ID ranges](#binary-id-ranges). Selections call it once per ≤ 1,000,000 IDs. |
| `getAssetQuerySections` / `get_asset_query_sections` | `sessionId: number` | `AssetQuerySectionsResult` (`{ status: "ready", session_id, total, sections: { t, start, count }[] }` or `{ status: "stale" }`) | Timeline sections built with the session ([ADR 0007](../adr/0007-session-timeline-sections.md)): `t` is the UTC month `yyyymm` of the bucket time, `0` for no date; a non-empty list is in session order and covers `0..total` without gaps, and a month over 5,000 rows is split (same `t`) only between buckets. A filtered session built them with its IDs; an unfiltered session reads them once from the sort-key index (same read transaction as the revision check) and keeps them. Same staleness check as page reads. `ready` with an empty `sections` list and `total > 0` means the sections are unavailable for this snapshot (a scan batch committed without a revision bump): the session stays valid and is not removed, the frontend hides the timeline and does not refresh ([ADR 0027](../adr/0027-unfiltered-sections-unavailable-instead-of-stale.md)). See [timeline sections](../subsystems/library-query-and-gallery.md#timeline-sections-and-scrubber). |
| `getAssetQueryPosition` / `get_asset_query_position` | `sessionId: number`, `assetId: number` | `AssetQueryPositionResult` | Returns `{ status: "resolved", index }`, `{ status: "missing" }`, or `{ status: "stale" }`. The index is zero-based in the full ordered snapshot; missing differs from expired, evicted, cleared, or dependency-stale sessions. No page or ID vector is transferred. |
| `getAssetSummariesByIds` / `get_asset_summaries_by_ids` | `assetIds: number[]` | `AssetSummary[]` | At most 256 IDs; larger requests reject. Reuses the database summary lookup, preserves requested ID order, and omits missing records. Selection hydration runs at most four batches concurrently. |
| `getAssetDetails` / `get_asset_details` | `assetId: number` | `AssetDetails \| null` | Returns the full path, size, and tags in addition to summary fields; unknown IDs return `null`. |
| `beginVideoOpen` / `begin_video_open` | none | `requestId: number` | Reserves a monotonically increasing open request and supersedes previous pending work. |
| `cancelVideoOpen` / `cancel_video_open` | `requestId` | `void` | Invalidates pending work and closes any session committed by that request. Repeated cancellation is safe. |
| `setVideoControlLabels` / `set_video_control_labels` | `sessionId`, localized `controlLabels` | `void` | Updates labels on the current GTK surface without reopening playback. |
| `openVideo` / `open_video` | `assetId`, `requestId`, finite non-negative `bounds`, localized `controlLabels`, `onEvent: Channel<MpvVideoEvent>` | `sessionId: number` | Resolves and authorizes the canonical video path in a blocking worker, supersedes an older pending/open session, opens it in process-wide libmpv, positions the GTK video surface, and labels its native controls. The frontend never supplies a path. |
| `setVideoBounds` / `set_video_bounds` | `sessionId`, finite non-negative `bounds` | `void` | Rejects stale sessions and moves/resizes the native surface using WebView coordinates and scale factor. |
| `controlVideo` / `control_video` | `sessionId`, tagged `command` | `void` | Rejects stale sessions. Playback commands are queued to the worker; execution failure arrives as `controlError`; property reads keep the snapshot authoritative. Supports play, pause, `togglePause`, non-negative absolute seek, `seekRelative` with finite signed `seconds`, volume 0–1, mute, rate 0.25–4, nonblank track IDs, and native-window fullscreen set/toggle. Pause toggles and relative seeks use current libmpv state on the worker, preserve pause state when seeking, and bound seeks to the media timeline. Fullscreen completes on the GTK thread before acknowledgement; its toggle computes the next state atomically there. |
| `closeVideo` / `close_video` | `sessionId` | `void` | Invalidates the current session, queues decoder stop, hides its GTK surface, and exits its fullscreen. Closing retired sessions is an idempotent no-op. |
| `listTags` / `list_tags` | `query: string`, `offset: number`, `limit: number` | `TagListPage` | Trims the search query, matches tag names case-insensitively, clamps offset to at least 0, and clamps limit to 1–200. |
| `getStartupPopularTags` / `get_startup_popular_tags` | none | `string[]` | Returns up to ten cached tag names, ordered by startup assignment count descending then name ascending with SQLite NOCASE ties. Reads immutable process state without SQLite or workflow locks. Captured after initialization/recovery and before startup scanning; remains fixed through frontend reload, restore, and clear until process restart. Calculation failure logs and stores an empty list. |

`SearchMetaFilter` is a discriminated input union:

- `{ type: "hasNoTags", tagCount: number }` requires a non-negative count.
- `{ type: "groupName", groupName: string }` trims the group name and rejects an empty result.

For both session and legacy queries, tags are trimmed, lowercased, de-duplicated in first-seen order, and empty values are removed. Media kind is trimmed and lowercased; only `image`, `gif`, and `video` survive, so an unknown kind currently behaves like no kind filter rather than producing an error.

## Asset mutations and duplicate operations

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `setAssetTags` / `set_asset_tags` | `assetId`, `tags: string[]` | `SetAssetTagsSummary` | Atomically replaces normalized tags and conditionally bumps revision. Missing IDs reject. A canonical no-op returns `changed: false`; repairing legacy tag spelling, duplicate normalized tag rows, or a stale `tag_count` returns `changed: true` and bumps revision. |
| `mergeAssetTagsBulk` / `merge_asset_tags_bulk` | `assetIds: number[]`, `tags: string[]` | `BulkTagMergeSummary` | Atomically merges normalized tags and revision, skips missing IDs, and returns canonical tags/results for processed IDs. |
| `applySelectionMutation` / `apply_selection_mutation` | `selection: { sessionId, ranges: [start, end][], excluded: number[], included: number[] }`, `mutation: { type: "mergeTags", tags } \| { type: "toggleFavorite" } \| { type: "setMediaGroup", mediaGroupKey: string \| null }` | `SelectionMutationResult` | Bulk write on a [selection spec](#selection-specs) ([ADR 0008](../adr/0008-selection-spec-resolved-in-rust.md)). Ranges are inclusive session positions, ascending and non-overlapping, else the command rejects. Resolved on the session vector, then written in one IMMEDIATE transaction that first checks the session's dependency revision; a missing or changed session returns `{ status: "stale" }` and writes nothing. Tags are normalized and must not be empty. |
| `setAssetFavorite` / `set_asset_favorite` | `assetId`, `isFavorite: boolean` | `void` | Sets the favorite flag and bumps the library revision. |
| `toggleAssetsFavoriteBulk` / `toggle_assets_favorite_bulk` | `assetIds: number[]` | `BulkFavoriteSummary` | Deduplicates positive IDs and skips missing rows. Reads and updates in one IMMEDIATE transaction on a blocking worker: removes favorites if all existing selected rows are favorites, otherwise favorites them all. Returns processed IDs and the common resulting value; bumps revision once for a nonempty processed set. |
| `setAssetMediaGroup` / `set_asset_media_group` | `assetId`, `mediaGroupKey: string \| null`, `mediaGroupOrder: number \| null` | `void` | Trims the key and maps blank to `null`; non-finite order is discarded as `null`; bumps the revision. |
| `setAssetsMediaGroupBulk` / `set_assets_media_group_bulk` | `updates: { assetId, mediaGroupOrder }[]`, `mediaGroupKey: string \| null` | `BulkMediaGroupSummary` | Drops non-positive/duplicate IDs, trims blank key to `null`, maps non-finite orders to `null`, and bumps the revision only when rows changed. The returned key is the normalized value; `processed_asset_ids` is collected inside the existing transaction, including unchanged existing rows. Omitted IDs were not processed. |
| `deleteAsset` / `delete_asset` | `assetId` | `DeleteAssetSummary` | Rejects an unknown ID. An existing source is moved to same-directory staging before one DB/revision transaction; a DB failure restores it. A missing source commits stale metadata removal with `source_status: "missing"`; failed final staged cleanup returns `cleanup_pending` and a recovery path. Never waits for a scan: while `scan_lock` is held (a scan, root removal, clear-library) it rejects at once with `i18n:common.libraryBusy` and changes nothing ([ADR 0022](../adr/0022-lightbox-delete-fails-fast-during-a-scan.md)). |
| `findDuplicateAssets` / `find_duplicate_assets` | none | `DuplicateScanSummary` | Groups database assets by duplicate file name and emits duplicate-scan progress. It does not hash file contents. |
| `applyDuplicateResolutionBatch` / `apply_duplicate_resolution_batch` | `input: { scanRevision, changes }`; each tagged change includes `assetId`, `expectedPath`, `expectedRecordVersion`, and rename also has `newFileName` | `DuplicateResolutionBatchSummary` | Validates the complete snapshot, IDs, versions, names, filesystem and DB collisions, and touched duplicate groups before mutation. Runs once under combined locks, stages all sources, commits one DB transaction/revision, and returns per-item commit, rollback, or recovery state. Rename targets that are another batch source are rejected. |

Both tag-save summaries require `vocabulary_changed: boolean`. It is true when the
transaction inserted, deleted, renamed, or merged stored tag names; bulk results OR
the per-asset flags. Existing-name assignments, shared-name removals, count-only
repairs, canonical no-ops, and empty/missing-only bulk results return false. This
flag does not replace `changed`, `query_impact`, or `revision`. Frontend and backend
ship this required field together.

The single-item favorite and group setters do not validate that an ID affected a row before resolving. Single tag replacement rejects a missing ID; bulk tag merge skips missing IDs and reports processed IDs. See [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md) and [lightbox](../subsystems/lightbox.md) for the user workflows built on these calls.

Single and bulk tag summaries require `query_impact`, preserving all existing fields and commands:

```ts
type TagQueryImpact =
  | { type: "none" }
  | { type: "tags"; changed_tags: string[]; tag_count_changed: boolean }
  | { type: "all" };
```

Names are sorted, unique, and normalized. Impact is computed from actual persisted changes and returned only after commit. Repairs use `all`. Favorite API signatures are unchanged.

## Scan roots and scanning

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `scanFolder` / `scan_folder` | `path: string` | `ScanSummary` | Normalizes and validates an existing directory, persists it as a root, then scans it. Runs blocking work off the async runtime and emits scan progress. |
| `listScanRoots` / `list_scan_roots` | none | `ScanRoot[]` | Sorts roots by available filesystem creation time, newest first; ties and missing-time entries sort lexicographically. This is not database insertion order. |
| `addScanRoot` / `add_scan_root` | `path: string` | `void` | Normalizes and validates an existing directory before persisting it; it does not scan. |
| `removeScanRoot` / `remove_scan_root` | `path: string` | `RemoveRootSummary` | Normalizes the path, removes the root and globally orphaned assets, conditionally bumps revision when assets were removed, and best-effort deletes their recorded thumbnails. |
| `rescanAllRoots` / `rescan_all_roots` | none | `ScanSummary` | Scans all stored roots; no roots yields a complete, zero-count summary and a `scan-empty` event. Runs blocking work off the async runtime. |

`setScanRootAutoScan` / `set_scan_root_auto_scan` accepts `{ path: string, enabled: boolean }` and returns void. It updates only an existing root's saved preference, including unavailable folders; an unknown root rejects. It does not scan or change the library revision.

`scanStartupRoots` / `scan_startup_roots` accepts no arguments and returns `ScanSummary | null`. It consumes the enabled-root snapshot captured by the backend at launch exactly once per process, even on failure. It runs through the existing scan lock, blocking pool, and progress events. Concurrent calls await the same process-owned scan; completed calls replay the retained summary or string rejection. Null means the launch snapshot had no enabled roots, not that another caller consumed it. Dropping an observer does not cancel work or completion publication. The frontend registers its progress listener before invoking this command, but completion does not depend on event delivery. Each mounted runtime requests it once, refreshes its query and roots after success or failure, and skips those follow-ups if disposed. A retained result can cause an extra refresh after a later remount; it never launches another scan.

Root normalization trims whitespace and removes trailing `/` except for the filesystem root `/`. It does not convert slash direction or canonicalize paths. `ScanSummary` always contains `completion`, `indexed`, `removed`, and `failed`; partial discovery/indexing failures produce `completion: "partial"` and preserve stale rows for the incomplete root.

## Thumbnails

On-demand thumbnails are not a command. Tiles load `thumb://localhost/<id>?v=<thumb_version>[&c=1][&p=1][&r=<n>]`, a custom URI scheme registered on the builder (`register_asynchronous_uri_scheme_protocol`) and built only by `thumbnailUrl` in `src/platform/tauri/thumbnails.ts`. The scheme accepts only a positive numeric ID, the 16-hex `thumb_version` token from `AssetSummary` or `DuplicateAsset`, `c=1` (serve a cached file only, never render), `p=1` (prefetch: render behind visible requests) and a retry counter `r`; anything else is `400` and paths are never accepted. Responses are `200 image/jpeg` (`immutable` when the served version matches `v`, otherwise `no-store`), `404` (no asset, missing source, failed render, or nothing cached for `c=1`) or `503` with `Retry-After` (state not ready, admission error, full queue). Each carries `Access-Control-Allow-Origin` for the page origin. Behaviour, caching and failure policy: [thumbnails](../subsystems/thumbnails.md#the-thumb-protocol), [ADR 0006](../adr/0006-thumb-uri-scheme.md). The former `ensure_thumbnails` channel command, its `ThumbnailStreamEvent` messages and the unregistered single-asset/page helpers were removed.

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `getVideoToolStatus` / `get_video_tool_status` | none | `VideoToolStatus` | Runs bounded executable availability checks on the blocking pool, without a workflow lock. Reports separate ffmpeg/ffprobe booleans; it does not test libmpv playback or decoding a particular file. |
| `renderAllThumbnails` / `render_all_thumbnails` | none | `ThumbnailRenderSummary` | Starts one process-wide bulk run; a concurrent bulk run rejects. Previously recorded failures are counted as `skipped_failed`. |
| `renderFailedThumbnails` / `render_failed_thumbnails` | none | `ThumbnailRenderSummary` | Same bulk-run guard, but retries only recorded failures and does not skip them. |
| `cancelRenderAllThumbnails` / `cancel_render_all_thumbnails` | none | `boolean` | Returns `true` and sets the cancellation flag only while either bulk mode is running; already completed ready results are retained. |
| `clearAllThumbnails` / `clear_all_thumbnails` | none | `number` | Clears database thumbnail paths and failure markers, exclusively blocks thumbnail readers, best-effort deletes referenced and obsolete files below the owned thumbnail directory, emits progress, and returns the number of files actually removed. |

## Import, export, and destructive data operations

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `exportTagsCsv` / `export_tags_csv` | `path: string` | `CsvExportSummary` | Trims and rejects an empty or protected target, then creates missing parent directories, and atomically publishes `file_name,tags,favorite,media_group_key,media_group_order` through a synced sibling temporary file. |
| `importTagsCsv` / `import_tags_csv` | `path: string` | `CsvImportSummary` | Opens a regular file, rejects more than 4 GiB while reading at most one byte past that limit, and requires exactly one of each standard header. It parses and validates records into disk-backed staging, then matches assets by the shared Unicode-lowercase basename key, merges normalized tags, updates favorite/group fields, and conditionally bumps revision in the same all-or-nothing transaction. |
| `exportDbBundle` / `export_db_bundle` | `path: string` | `DbBundleExportSummary` | Trims and rejects an empty or protected target, then creates missing parent directories, creates a ZIP from one SQLite Backup API snapshot plus thumbnail files, syncs a unique temporary archive, and atomically publishes it. Current exports contain no WAL/SHM entry. |
| `inspectDbBundle` / `inspect_db_bundle` | `path: string` | `DbBundleInspection` | Validates archive limits, manifest compatibility, SQLite integrity/schema, and manifest/database roots without writing the candidate database; reports roots requiring Windows-to-Linux mapping. |
| `importDbBundle` / `import_db_bundle` | `path: string`, `rootMappings: { sourceRoot, targetRoot }[]` | `DbBundleImportSummary` | Repeats full archive/database validation, migrates only accepted legacy staging, validates/rewrites every media and thumbnail path, then performs a maintenance-gated journaled replacement. Linux rejects Windows roots without complete mappings and detects mapped path collisions. |
| `clearLibraryData` / `clear_library_data` | none | `ClearLibrarySummary` | Removes assets and roots, bumps the revision, best-effort deletes thumbnail files, and emits library-clear progress. |

These operations' locking, transaction, archive, rollback, and filesystem guarantees are documented in [data safety and portability](../subsystems/data-safety-and-portability.md).

## Native window

| Frontend wrapper / command | Arguments sent by the wrapper | Return type | Important semantics |
| --- | --- | --- | --- |
| `syncNativeWindowTheme` / `sync_window_theme` | `theme: "light" \| "dark"` | `void` | Rejects any other string and sets the native theme and background. |

### Selection specs

`apply_selection_mutation` returns:

```ts
type SelectionMutationResult =
  | {
      status: "ready";
      processed_assets: number; // existing selected assets
      updated_assets: number;   // assets whose stored value changed
      revision: number;         // global revision after the write
      query_impact: TagQueryImpact; // tag merge only; `none` otherwise
      vocabulary_changed: boolean;
      is_favorite: boolean | null;     // toggleFavorite only
      media_group_key: string | null;  // setMediaGroup only (normalized)
    }
  | { status: "stale" };
```

The resolved asset list is the range positions minus `excluded`, in session order, followed by the `included` IDs that the ranges did not already cover. `included` IDs may come from an earlier query; missing assets are skipped, as in the ID-based commands. `setMediaGroup` assigns orders `1..N` in that list order. The spec has no revision argument: like pages, a session is valid while its dependency revision is unchanged, so a write that does not affect the session's dependencies (for example the selection's own tag merge in an unfiltered view) does not invalidate it. A `stale` result is a successful response, not an invoke error.

## Query session states

A successful `start_asset_query` returns:

```ts
{
  status: "ready";
  session_id: number;
  revision: number;
  total: number;
  offset: number; // page containing `anchorOffset`, 0 without an anchor
  items: AssetSummary[];
}
```

Sessions bind an ordered asset-ID snapshot to normalized filters and their dependency revision. The public `revision` remains the global revision of the read snapshot. See [the query manager lifecycle](../subsystems/library-query-and-gallery.md#backend-session-and-page-contract) for snapshot construction, cancellation, reuse, and cache limits.

- `superseded` is a successful start result, not an invoke error. An equal or lower owner generation, or a request whose backend registration token is no longer latest, returns it, including on the cache-hit path. `createAssetQuery` ignores responses from obsolete local generations. If its current start is superseded, it publishes a query error with explicit Retry and retains the old rows without an active session. It does not retry automatically.
- `stale` is a successful page, position, sections, or ID-range result, not an invoke error. It means the session ID is missing, expired, evicted, or bound to an older dependency revision. A dependency revision mismatch also removes that session. `createAssetQuery` responds by scheduling a debounced fresh query.
- `ready` pages preserve the session order and report the effective (possibly end-clamped) offset. Relevant dependency changes and full invalidations make later pages stale. Unrelated metadata edits keep the session usable and appear in newly materialized summaries.

### Binary ID ranges

`get_asset_query_id_range` is the only query command with a binary body (`tauri::ipc::Response`); the frontend receives an `ArrayBuffer` instead of JSON ([ADR 0005](../adr/0005-binary-id-ranges-and-anchored-refresh.md)):

- `ready`: little-endian IEEE-754 f64 values `[count, id_0, …, id_{count-1}]`, so a ready empty range is eight bytes. `count` equals the clamped range length.
- `stale`: an empty body (zero bytes).

IDs are sent as f64 because every SQLite asset ID the application creates is far below 2^53, where f64 is exact; the frontend reads them through a `Float64Array` view without BigInt conversion. The backend rejects the whole range with a text error if any ID is negative or above 2^53. `decodeAssetIdRange` in `src/platform/tauri/assets.ts` rejects bodies whose length is not a multiple of eight or whose count header disagrees with the length, copies unaligned views, and falls back to `DataView` on a big-endian host.

An authoritative query exceeding the 64 MiB retained-ID budget rejects with a text error asking the user to narrow the search. It is not a superseded result; a fitting query can subsequently succeed. Frontend code uses ordinary query error handling and does not branch on the error wording.

Because supersession and the cache are process-wide, requests from another window or independent caller can supersede or evict this window's work. Pooled database connections are acquired with a five-second bounded wait; exhaustion rejects the command with a text `database pool is busy` failure instead of blocking forever.

## Broadcast progress

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
- thumbnail clear: `thumbs-clear`;
- library clear: `library-clear`, `library-clear-done`; and
- duplicates: `duplicates-scan`, `duplicates-scan-done`.

Consumers must filter by phase because the event name is shared and broadcasts are not correlated to a command invocation. `createSettingsOperationRunner` subscribes only for the duration of an operation and applies a phase matcher. Messages are display text, not stable machine-readable error data. Most emit sites intentionally discard delivery errors; the command result is authoritative for operation completion.

`cancel_render_all_thumbnails` returns an independent boolean reply, which can reach the frontend after the render command has completed. The scan controller captures a synchronous render token when sending Stop and checks it before publishing either success or rejection. A retired reply cannot replace a final result or clear a newer operation's progress or pending Stop state. The token is local frontend state; progress events and cancellation payloads remain uncorrelated on the wire.

## Native video input channel

`open_video`'s existing `Channel<MpvVideoEvent>` carries `{ session_id, type: "escape" }`
when windowed GTK video controls have keyboard focus. It is an input intent, not a
close command. GTK consumes speed-menu and fullscreen Escape first. WebView-focus
Escape stays on the DOM path. The Rust service sends only for the current session;
`createNativeVideoSession` checks the returned session ID and owner lifetime before
calling the lightbox callback. An event arriving before open completes is buffered
like the other channel event types. Retired or disposed sessions cannot dismiss a
replacement preview. The frontend layer owner decides whether to dismiss inline
confirmation, a narrow drawer, or the preview, and blocks input during deletion or
while a higher registered layer is active.

## Boundary validation and normalization summary

| Input | Boundary behavior |
| --- | --- |
| Session page size/limit | Clamp to 1–256; first-page wrapper default is 128. |
| Legacy asset page | Offset at least 0; limit 1–500. |
| Tag page | Offset at least 0; limit 1–200; query trimmed and case-insensitive. |
| Query/tag arrays | Trim, Unicode-lowercase, remove empty strings, de-duplicate preserving first occurrence, and reject whitespace, comma, semicolon, or control characters inside a tag. |
| Query media kind | Trim/lowercase; unsupported values become no filter. |
| Meta-filter | Reject negative `tagCount` and blank `groupName`; require the tagged camelCase shape. |
| Bulk asset IDs | Tag/group mutation APIs drop non-positive and duplicate IDs. `thumb://` accepts one positive ID per request. |
| Media group | Blank key becomes `null`; non-finite order becomes `null`. |
| Root path | Trim and remove trailing `/` except for filesystem root; add/scan requires an existing directory. |
| CSV/bundle paths | Trim; exports reject empty paths, the active profile, indexed roots, and indexed source files, and create missing parents only after those checks pass ([data safety](../subsystems/data-safety-and-portability.md#export-target-validation)). Imports require an existing file. Bundle inspection/import reject unsafe or duplicate recognized ZIP paths, symbolic-link entries, resource-limit violations, missing `media.db`, incompatible manifests/databases, and unsafe persisted media/thumbnail paths. |
| Rename | File name only; rejects blank/dot names, separators, control/listed Windows-invalid characters, trailing dot/space, reserved device names, same path, and occupied filesystem or indexed destinations. |
| Theme | Exactly `light` or `dark`. |
| Video bounds/control | Bounds must be finite and non-negative. Seek is non-negative; volume is 0–1; rate is 0.25–4; selected track IDs must be nonblank. Current-session checks reject late controls, bounds, and closes. |

Deserialization itself rejects missing required arguments, wrong JSON types, invalid tagged-union shapes, negative values sent to unsigned Rust parameters, and non-representable numbers before command logic runs. Frontend TypeScript types help normal callers but are not runtime validation for arbitrary IPC callers.

## Known limitations

- Rust and TypeScript payload types are maintained manually; there is no generated schema or compile-time cross-language parity check.
- `thumb://` requests cannot be cancelled: WebKitGTK gives the handler no signal when an `<img>` goes away, so a queued render still runs and answers a request nobody reads.
- IPC errors are text only. Consumers cannot reliably distinguish validation, not-found, busy, filesystem, database, worker, or platform failures except by message text.
- Numeric Rust IDs/counters are exposed as JavaScript `number` without an explicit safe-integer guard.
- Event delivery is best effort. `process-progress` sends commonly ignore delivery errors; events can be missed and must not be used as a commit signal.
- `process-progress` is a shared broadcast with free-form `phase` and human-readable `message`; it has no request ID, sequence number, or version.
- Favorite and media-group setters resolve even if an unknown asset ID changed zero rows. Tag replacement, delete, and rename reject unknown IDs; bulk tag/group operations skip missing IDs and report processed counts or IDs.
- Runtime validation is uneven: unsupported query kinds degrade to no filter and non-finite group order degrades to `null`.
- `src/platform/tauri/__tests__/api.test.ts` covers query-session payloads, tag mutation, native video session/channel payloads, and media-path conversion, but it does not exhaustively lock down every wrapper, response shape, or event.

## Safe contract-change checklist

When changing IPC:

1. Update the Rust command signature/model and the matching `src/platform/tauri` wrapper and `contracts` type in the same change. Confirm command argument casing, nested serde casing, optional/null behavior, enum tags, and all numeric types.
2. If adding or removing a command, update the `tauri::generate_handler!` registration in `src-tauri/src/lib.rs`. Search for direct `invoke` calls so the wrapper remains the single frontend seam.
3. Preserve existing command names and payload fields when compatibility is required. For a migration, add the replacement first, move consumers and tests, then explicitly remove the legacy command/event in a later breaking change. Do not silently repurpose ignored compatibility fields.
4. For query-visible writes, decide when to bump the library revision and test the resulting `ready`/`stale` behavior. Test cache reuse, expiry/eviction assumptions, limit/offset clamps, concurrent starts, and frontend generation rejection if those semantics change.
5. For `thumb://`, test the parser (no paths, numeric IDs, hex versions, known flags only), status codes, cache and CORS headers in `thumb_protocol.rs`, and the URL shape in `api.test.ts`; exercise real loading in desktop E2E.
6. For progress, treat the event name, payload fields, and phase strings as contract. Update phase matchers and translations together, and never make successful event delivery a prerequisite unless the API is intentionally redesigned.
7. Keep command errors text-only unless Rust and every consumer migrate together to a versioned structured error payload. Do not branch new code on current message wording.
8. Re-evaluate boundary validation, workflow locks, revision/cache invalidation, filesystem rollback, and destructive-operation safety. Follow the [system overview](system-overview.md) and the relevant subsystem document rather than duplicating those policies here.
9. Add focused frontend wrapper tests and Rust serialization/command/service tests. Run the frontend, backend, and real Tauri E2E suites in proportion to the change; an IPC signature change requires a real desktop-path check, not only mocked `invoke` tests.
10. Update this document in the same change, including the primary/legacy designation and Known limitations.

### Relevant tests

- `src/platform/tauri/__tests__/api.test.ts` checks selected invoke payloads (including `anchorOffset`), binary ID-range decoding (stale, empty, unaligned, byte-array and malformed bodies), native video channel delivery, and media-path conversion.
- `asset_query_service.rs` unit tests cover anchored starts on fresh and cached sessions, ID-range bounds, staleness and f64 encoding limits, and timeline sections (months, groups, no date, filters, staleness, JSON shape, a kept session without sections after an unbumped scan commit); `backend_integration` (`sections_during_a_scan`) repeats that case with a real scan.
- `selection_service.rs` pins the `apply_selection_mutation` payloads (camelCase spec and mutation, snake_case result); `backend_integration.rs` (`selection_spec`) covers resolution, exclusions, inclusions, all three mutations, stale and missing sessions writing nothing, and rejected ranges. `api.test.ts` checks the sections and selection-mutation invoke payloads.
- `src/library/__tests__/createLibrary.test.ts` exercises the session-query contract through the library model, including filters and pagination.
- `src/platform/tauri/__tests__/api.test.ts` checks the `thumb://` URL shape, including the cached-only and retry flags.
- `src/features/settings/__tests__/progressService.test.ts` locks down phase matching and progress-summary formatting; settings model tests cover subscription lifetimes and command workflows.
- Rust tests in `src-tauri/src/commands/assets.rs`, `scan.rs`, `thumbs.rs`, `video.rs`, and `import_export.rs` cover filename/root/CSV/video validation and cancellation behavior.
- Rust tests in `src-tauri/src/services/thumb_service.rs`, `scan_service.rs`, `backup_service.rs`, `asset_query_service.rs`, `video_source_service.rs`, and `video_player_service.rs`, plus `src-tauri/src/db.rs`, cover thumbnail results, partial scans, archive and video-source safety, query/session semantics, and persistence behavior.
- `src-tauri/tests/backend_integration.rs` and `backend_e2e.rs` cover file-backed cross-layer mutation/import workflows but do not exercise JavaScript serialization.
- `e2e/specs/*.e2e.js` exercises the registered commands through a real desktop WebView and is the strongest existing check for Rust/TypeScript integration.

## System browser links

Settings → About uses the official `@tauri-apps/plugin-opener` `openUrl` binding,
registered through `tauri_plugin_opener` at bootstrap. The main window's capability
allows `opener:allow-open-url` only for the repository configured in
`packaging/flatpak/publisher.json` and its subpages. The packaging regression checks
that these URLs stay synchronized. No `opener:default`, path-opening or arbitrary
application selection permission is granted. Automatic HTML-link handling is disabled.
The component handles rejected calls with a local message and a selectable URL.
