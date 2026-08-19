# App Errors And Reliability Hardening Plan

## Purpose

Prepare a future task for improving visible error handling, stale async request safety, thumbnail queue cleanup, i18n key reliability, and safer database bundle import behavior.

This is a planning artifact only. The later implementation task should make the code changes.

## Current State

- Frontend IPC is centralized in `src/api.ts`.
- Tauri command registration is in `src-tauri/src/main.rs`.
- Shared frontend domain types are in `src/types.ts`.
- Rust payload models are in `src-tauri/src/models.rs`.
- Progress events use the event name `process-progress`.
- Thumbnail completion events use the event name `thumbnail-ready`.
- Backend command modules live under `src-tauri/src/commands/*`.
- Backend workflow logic lives under `src-tauri/src/services/*`.
- SQL access lives in `src-tauri/src/db.rs`.
- Locale resources live in `src/i18n/locales/*.json`.

## Repo Rules To Respect

- Do not write or update tests unless Jan explicitly asks.
- Do not run tests, builds, or checks unless Jan explicitly asks.
- Do not use `bunx` for test execution.
- Preserve shared payload shapes unless the task explicitly requires a contract change.
- If a frontend-backend contract changes, update shared frontend types before command/service consumers.
- Keep Tauri commands thin: validate input, acquire state/locks, delegate workflow logic to services.
- Keep progress event name `process-progress`.
- Keep progress payload compatible with `ScanProgress { phase, processed, total, message }`.
- Preserve backend lock order: `scan_lock`, then `thumb_lock`.

## Fix I18n Reliability

1. Add missing `common.confirm` to every locale file in `src/i18n/locales`.
2. Add `gallery.loadError` to every locale file for visible gallery load failures.
3. Align `fr.json` and `cs.json` with the complete `en.json` key tree.
4. Use existing translated text when available.
5. Use English fallback text only where no translation exists.
6. Check Czech validation keys:
   - If code reads root `validation.*`, move or duplicate keys currently under `settings.validation.*` to root `validation.*`.
   - Remove the unused nested copy only after confirming no code path reads it.
7. Update `scripts/generate-locales.mjs` so supported languages include every `AppLanguage`, including `cs`.
8. Preserve interpolation placeholders exactly when generating or copying locale values.

## Harden Gallery And Library Loading

1. Extend internal library state with:

```ts
loadError: string | null;
```

2. Add an error prop to the gallery controller shape:

```ts
errorMessage: string | null;
```

3. In `useLibraryAssets`, wrap `listAssets` calls in `try/catch/finally`.
4. On load failure:
   - Store a user-visible load error.
   - Clear `loading` for the current request.
   - Avoid silently swallowing the error.
5. On successful load:
   - Clear `loadError`.
   - Update `assets`, `total`, `offset`, and thumbnail state as today.
6. Add a latest-request guard:
   - Increment a request sequence before each refresh/page load.
   - Only the newest request may mutate `assets`, `total`, `offset`, `thumbs`, `loading`, and `loadError`.
   - Stale responses must exit without changing current state.
7. Render `gallery.loadError` through the gallery UI, using:
   - `role="alert"`
   - DaisyUI error tokens
   - Existing layout conventions

## Harden Thumbnail Queue Lifecycle

1. Treat `listen("thumbnail-ready")` registration failure as non-fatal because `ensurePageThumbnails` still returns final batch results.
2. Initialize the listener cleanup handle as nullable.
3. Always run command cleanup even if listener registration fails.
4. Always remove batch IDs from `inFlightIdsRef` in cleanup.
5. On hook unmount:
   - Increment `generationRef`.
   - Clear timers.
   - Clear queued IDs.
   - Clear in-flight IDs.
   - Prevent stale async callbacks from mutating queue state.
6. Keep the `thumbnail-ready` event name and event payload behavior unchanged.

## Harden Database Bundle Import

Implement safer restore behavior inside `src-tauri/src/services/backup_service.rs`.

Required behavior:

1. Validate all archive entry paths before touching current app data.
2. Reject unsafe archive paths, path traversal, missing `media.db`, or invalid archive structure.
3. Extract archive contents to a staging directory under the app data parent.
4. Validate/open the staged database.
5. Run schema initialization on the staged database copy.
6. Move current database files and thumbnails to rollback paths before final replacement.
7. Include database sidecar files where relevant, such as WAL and SHM files.
8. Move staged database and thumbnails into place.
9. If final replacement fails:
   - Attempt rollback.
   - Return a clear error string.
10. Clean staging and rollback leftovers after success.

Keep command boundaries returning `Result<T, String>` and map internal errors with `.to_string()` at command boundaries.

## Verification

Only run commands when Jan explicitly authorizes them.

If Jan permits build verification:

```powershell
bun run build
```

If Jan explicitly asks for frontend tests, add or update focused Vitest coverage for:

- i18n key presence
- visible gallery load failure UI
- stale request guarding
- thumbnail listener failure cleanup

Then run:

```powershell
bun run test
```

If Jan explicitly asks for backend tests, add or update focused Cargo coverage for:

- failed DB bundle import rollback
- unsafe archive path rejection
- missing `media.db` rejection

Then run:

```powershell
bun run test:backend
```

## Acceptance Criteria

- Missing locale keys are filled consistently across supported languages.
- Gallery load failures are visible to the user.
- Stale asset loads cannot overwrite newer search/filter results.
- Thumbnail queue state cannot stay stuck after listener registration failure or hook unmount.
- Failed DB bundle imports preserve previous app data or return a clear rollback failure.
- No tests/checks were run unless Jan explicitly authorized them.
