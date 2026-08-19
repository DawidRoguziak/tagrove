# App Optimization And Codex Process Plan

## Purpose

Prepare a future task for improving runtime performance and repeatable optimization workflow in the media tagging app. This plan prioritizes measurable changes that preserve existing public contracts and UI behavior.

This is a planning artifact only. The later implementation task should make the code changes.

## Current State

- Backend boundaries are documented in `docs/architecture/system-overview.md`.
- Tauri commands are thin and live under `src-tauri/src/commands/*`.
- Backend workflow logic lives under `src-tauri/src/services/*`.
- SQL access lives in `src-tauri/src/db.rs`.
- Scanning uses worker threads and caps worker count at 12.
- Thumbnail generation uses `ThumbnailScheduler`.
- Gallery virtualization uses `@tanstack/react-virtual`.
- Frontend thumbnail queueing uses a batch size of 180.
- SQLite uses WAL mode and existing indexes.
- Startup may duplicate `library.refresh()` because initial shell loading and filter-driven refresh behavior can both request assets.

## Repo Rules To Respect

- Performance comes first, but preserve correctness and app responsiveness.
- Use multiple threads where appropriate and avoid blocking the UI thread.
- Do not write or update tests unless Jan explicitly asks.
- Do not run tests, builds, or checks unless Jan explicitly asks.
- Do not use `bunx` for test execution.
- Preserve existing frontend-backend payload contracts unless Jan explicitly approves a contract change.
- Keep `process-progress` as the progress event name.
- Keep progress payload compatible with `ScanProgress { phase, processed, total, message }`.
- Preserve backend lock order: `scan_lock`, then `thumb_lock`.
- Keep media group ordering behavior intact.

## Baseline First

Before changing behavior in the future execution task, collect lightweight baseline measurements:

- Scan timing by phase.
- Thumbnail throughput.
- `list_assets` query time.
- Gallery scroll/render behavior.
- Cold thumbnail-cache behavior.
- Rescan behavior for unchanged image, GIF, and video files.

Diagnostics must be development-only or behind a private environment flag. Do not add public command payload fields for instrumentation unless Jan explicitly asks.

The final implementation note for the later task must include before/after measurements or explain why measurement was not possible.

## React And Frontend Optimization

1. Remove duplicate initial library refresh in `useAppShellController.ts`.
   - Choose one owner for the first asset load.
   - Keep filter/search refresh behavior after initialization.

2. Make independent refresh work parallel.
   - In `useLibraryBrowser.ts`, fetch assets and known tags with `Promise.all` when neither depends on the other.
   - Preserve existing state semantics and error handling.

3. Reduce repeated work in gallery rendering.
   - In `GalleryGrid.tsx`, hoist repeated `videoChipLabel` and `gifChipLabel` work out of the virtual item map.
   - Do not change gallery layout, virtualization, or media tile behavior.

4. Reduce resize/effect churn.
   - In `useGalleryVirtualGrid.ts`, update `ResizeObserver` state only when rounded width or height actually changes.
   - Derive a stable virtual range signature so prefetch effects do not churn only because of array identity.

5. Preserve existing behavior:
   - Keep `@tanstack/react-virtual`.
   - Keep GIF animation threshold behavior.
   - Keep thumbnail queue semantics and batch size unless measurements prove a change is needed.

## Scan And Index Optimization

1. Add a DB helper that fetches existing asset fingerprints by path in safe chunks.
2. During scan, compare current file metadata with existing fingerprints.
3. For unchanged files:
   - Skip expensive image dimension reads when existing dimensions are available.
   - Skip expensive ffprobe duration probes when existing video metadata is still valid.
   - Still update `indexed_at` so cleanup logic knows the file is present.
4. Preserve existing thumbnail paths through current upsert behavior.
5. Keep worker count cap unchanged for the first optimization pass.
6. Tune worker count only after measurements show a concrete bottleneck and no Windows oversubscription problem.
7. Keep progress event names and payload shapes unchanged.

## Thumbnail Optimization

1. Add a DB helper that fetches thumbnail asset rows for a batch of asset IDs in safe chunks.
2. Preserve requested ID mapping/order where the frontend expects it.
3. Update `ensure_page_thumbnails` to use the batch helper instead of one DB read per asset.
4. Keep scheduler priority behavior unchanged.
5. Keep thumbnail deduplication behavior unchanged.
6. Keep `thumbnail-ready` event behavior unchanged.
7. Keep frontend batch size at 180 for the first pass unless measurements show it should change.

## SQL Optimization

1. Profile before adding indexes.
2. Consider indexes only when measurements confirm benefit for common filters:
   - media kind with modified/id ordering
   - favorites with modified/id ordering
   - normalized group key or group-name lookup if group search is slow
3. Preserve behavior for:
   - tag include filters
   - tag exclude filters
   - no-tags meta filter
   - group-name meta filter
   - favorites-only filter
   - pagination
   - media group ordering
4. Do not weaken grouped media ordering semantics in `list_assets` or related query helpers.

## Codex And Developer Process

Add a short performance workflow note to `AGENTS.md` or another repo documentation file during the later implementation task.

The note should say:

- Baseline before optimizing.
- Isolate one hot path at a time.
- Keep command and shared type contracts stable unless a contract change is explicitly requested.
- Prefer parallel/off-main-thread work when it improves responsiveness.
- Do not write tests, update tests, or run test/build commands unless Jan explicitly asks.
- Report before/after measurements or explain why measurement was not possible.

## Verification

Only run commands when Jan explicitly authorizes them.

If Jan explicitly authorizes targeted checks, prefer:

```powershell
bun run build
bun run test
bun run test:backend
```

If Jan requests broad validation, run:

```powershell
bun run test:all
```

Manual scenarios for later validation:

- Large rescan with unchanged images and videos.
- Page thumbnail generation on a cold cache.
- Gallery scroll with many images, GIFs, and videos.
- Tag include/exclude search.
- Meta filters.
- Favorites-only filter.
- Grouped media ordering.

## Acceptance Criteria

- Duplicate startup asset loading is removed.
- Independent frontend refresh work runs in parallel.
- Unchanged-file rescans avoid unnecessary expensive metadata probing.
- Page thumbnail generation avoids one DB read per asset.
- SQL/index changes are measurement-driven and preserve existing behavior.
- Codex/developer performance workflow is documented.
- No tests/checks were run unless Jan explicitly authorized them.
