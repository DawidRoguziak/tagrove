# 0018: Render all keeps the cached thumbnail of an unreachable source and writes only rows that change

- Status: Accepted
- Date: 2026-11-11
- Areas: thumbnails, database (thumbnail writes)
- Related: [thumbnails](../subsystems/thumbnails.md#bulk-render-retry-and-cancellation), [scanning](../subsystems/scanning-and-indexing.md), [ADR 0006](0006-thumb-uri-scheme.md)

## Context

`render_bulk_thumbnails_inner` (`src-tauri/src/services/thumb_service.rs`) checked `source_path.exists()` before anything else. For a missing source it removed the asset's recorded thumbnail file, and the queued failure write set `thumb_path` to NULL and recorded a `source file not found` marker for the unchanged `record_version`. `Path::exists` is false for an unmounted drive and for an unreadable folder (EACCES) alike, and partial scans do not prune rows, so running Render all while a drive was offline deleted every cached thumbnail of that drive, including video thumbnails that take up to 3 x 45 s each. After the drive came back the deterministic target was gone and the marker was current, so Render all skipped those assets as previously failed; only Retry failed or browsing them one by one rebuilt them. The `thumb://` path does the opposite: it serves a recorded file without looking at the source.

The same loop also published a Ready write for every asset whose target existed, even when the row already recorded that path and had no failure marker. On a rendered library each such row cost a compare-and-set `UPDATE` that changed nothing, two re-reads and a marker `DELETE`, in one `IMMEDIATE` transaction per 256 rows.

## Options considered

1. **Keep the file and path, but still record the failure marker.** The summary keeps reporting the offline assets as failed. But the demand path clears the marker as soon as a tile serves the file, and when the recorded file is not the deterministic target (an earlier cache version) the marker makes the next Render all skip the asset after the source is back, which is the original poisoning.
2. **Skip the asset without counting it.** Keeps the cache, but the summary has no field for it, and `processed` would no longer reach `total` without an IPC change.
3. **Treat a usable recorded thumbnail as ready.** The asset is counted as generated, its file and path stay, no marker is written and a stale one is cleared. This is what the demand path already does.
4. **Detect EACCES separately from ENOENT.** Does not help an unmounted drive, which is ENOENT.

For the redundant writes: always publishing (current behavior), or publishing only when the stored path differs or a marker exists. The candidate page already returns both, so the second needs no extra query.

## Decision

Option 3, with writes limited to rows that change:

- When the source is not reachable and the row's `thumb_path` is a regular file (not a symbolic link) inside the canonical thumbnail root, Render all and Retry failed count it as generated and keep the file and path. A Ready write of that recorded path is published only when a failure marker for the current `record_version` exists. No file is removed. The root check matters because `thumb_path` is untrusted and a Ready write dropped by the clear epoch removes its path; a recorded path outside the root falls through to the missing-source failure, which clears the path without touching the file.
- A current target that exists is counted as generated, for a reachable or unreachable source. It is published only when the row does not already record that path or has a failure marker.
- Only a missing source without a usable file records `source file not found`, as before. It no longer calls `remove_thumbnail_file`, since in that branch no file is recorded.
- Rows counted as generated without a write are moved to the stale count when the clear epoch changed after their page was read, matching the written rows the epoch check drops.

## Consequences

- An offline drive keeps its gallery tiles and costs no re-render when it returns. The summary reports those assets as ready, not as failed; a missing source is visible through its blank tile only when nothing was cached.
- A fully rendered library runs Render all without any SQLite write; tested by holding the write lock from another connection (`render_all_does_not_rewrite_rows_whose_thumbnail_is_current`). Repairing a lost path or a stale marker is unchanged.
- An asset that was never rendered and whose source is offline still gets a marker and is skipped by the next Render all after the source returns; the demand path's one retry per process and Retry failed still cover it.
- No schema, IPC or library-revision change: thumbnail writes never bump the revision, and the summary fields are the same.
- Tests: `thumb_service.rs` unit tests for an unreadable folder, a stale marker on an offline source, a missing source without a file, current rows under a held write lock, and a clear between page and publication; `tests/thumbnail_bulk.rs` takes a folder offline and back with the real image renderer.
