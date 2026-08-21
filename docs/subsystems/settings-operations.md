# Settings operations

This page describes the settings UI and its frontend orchestration as implemented today. The React hooks, components, API wrappers, and executable tests remain authoritative. Backend command payloads and event serialization belong in the [IPC contract](../architecture/ipc-contract.md); scan/index rules belong in [scanning and indexing](scanning-and-indexing.md); thumbnail scheduling and cancellation belong in [thumbnails](thumbnails.md); archive replacement, rollback, CSV semantics, and other import/export safety properties belong in [data safety and portability](data-safety-and-portability.md).

## Composition and state ownership

`useAppShellController` constructs `useSettingsActions` even when the gallery is visible. The settings action state therefore has shell lifetime: it survives switching between the gallery and the lazy-loaded `AppSettingsView`, but not a complete app unmount or restart. On shell mount, scan roots are hydrated alongside known tags. `useSettingsView` independently owns whether the full-page settings view is open; Back and its window-level Escape listener return to the gallery.

`AppSettingsView` supplies the sticky page header and renders `SettingsPanel` in full-view mode. `SettingsPanel` is the rendering/composition boundary for five sections:

1. `AppearanceSection` changes theme and language through its own controller. These preferences do not use the operation runner.
2. `ScanSettingsSection` lists roots and exposes add, remove, per-root/all-root rescan, bulk thumbnail render, retry, and stop controls.
3. `ImportExportSection` exposes CSV and database-bundle import/export.
4. `DuplicatesSection` starts the duplicate scan and shows the last non-zero result.
5. `DangerZoneSection` opens the clear-library confirmation.

The panel also mounts the remove-root, clear-library, duplicate resolver, duplicate-delete, and database-import confirmation dialogs. Production composition passes the typed `appearance`, `scan`, `importExport`, `duplicates`, and `dangerZone` controller groups. The flat props and no-op defaults in `SettingsPanel` are a compatibility/test seam, not an additional state owner.

`useSettingsActions` composes one shared `useSettingsOperationRunner` with four focused hooks:

| Hook | Owned state and responsibility |
| --- | --- |
| `useScanSettingsActions` | Scan-root list, thumbnail bulk/cancel flags, remove-root candidate, and scan/thumbnail actions. |
| `useImportExportSettingsActions` | File-dialog orchestration and the pending database-bundle source awaiting overwrite confirmation. |
| `useDuplicateSettingsActions` | Resolver visibility, pending delete confirmation, duplicate snapshot/revision, and one backend batch application. |
| `useDangerZoneSettingsActions` | Clear-library confirmation and action. |

Duplicate thumbnails reuse the library browser's thumbnail map, rendering-ID map, and queue callback. Cross-feature refresh/reset callbacks also come from `useLibraryBrowser` and `useLibraryLifecycle`; settings code does not maintain a second asset cache.

## Exclusive operation runner

The runner serializes settings work within the mounted frontend. A synchronous `operationLockRef` rejects a second call immediately, before React can publish the rendered `isOperationLocked` state. The rendered boolean is passed to every operation section, so their normal action buttons are disabled together. This is a UI-level lock only; backend concurrency and workflow locks are described in the [system overview](../architecture/system-overview.md).

The lock is global, but operation feedback is per section. Each of `scan`, `importExport`, `duplicates`, and `danger` has an independent `SectionOperationState`:

```ts
{
  loading: boolean;
  message: string;
  progress: ScanProgress | null;
}
```

Each starts with a translated section-specific ready message. Running an operation changes only the initiating section to `loading: true`, installs its pending message, and clears its previous progress. Status messages in other sections remain unchanged. `SectionOperationStatus` renders a `role="status"` spinner while `loading` is true. When progress exists it also renders `processed/total`; a zero total produces an indeterminate progress bar with `max=1` and no `value`.

The runner lifecycle is:

1. Return without doing anything if the ref lock is already set.
2. Set the ref lock and rendered global lock.
3. Unless `setGlobalLoading: false` was requested, set the library browser's global loading state to true.
4. Mark the initiating section loading, set its pending message, and clear progress.
5. If configured, await registration of a `process-progress` listener.
6. Await the supplied action.
7. On any listener/action/follow-up error, replace that section's message with `<translated error prefix>: <translated backend text>` and clear progress. Errors are presented, not rethrown by the runner.
8. In `finally`, unregister the progress listener, clear that section's loading flag, clear global loading when it was set, and release both locks. The final message remains visible.

Folder selection and duplicate scans set `setGlobalLoading: false`; duplicate change application and every other runner-backed action use global loading. Duplicate operations still acquire the shared settings lock and show duplicate-section loading. Thumbnail cancellation deliberately bypasses `runExclusiveOperation`: it must remain callable while the bulk thumbnail operation owns the lock. Its separate `cancelThumbnailRunning` guard prevents duplicate stop requests, and the visible Stop button is disabled only by that flag.

Closing the settings page does not cancel an operation because the settings hooks remain mounted in the shell. Reopening the page exposes the same lock, messages, progress, roots, and duplicate results.

## Progress events and message translation

Operations that report progress subscribe to the single `process-progress` event before invoking their API wrapper. The payload is `ScanProgress`: `phase`, `processed`, `total`, and `message`. Each operation accepts only its own phases:

| Operation | Matcher |
| --- | --- |
| Rescan one root or all roots | Exact `counting`, `scanning`, `cleanup`, `scan-empty`, or `scan-skip`. |
| Render all thumbnails or retry failed thumbnails | `phase.startsWith("thumbs")`. |
| Clear library | `phase.startsWith("library-clear")`. |
| Find duplicates | `phase.startsWith("duplicates-scan")`. |

An irrelevant phase is ignored. Recognized phase names are translated by `progressService` and take precedence over the payload's `message`. The known translations cover scan counting/scanning/cleanup/empty/skip, thumbnail start/work/done and failed-retry variants, library-clear work/done, and duplicate-scan work/done. Progress phases that are accepted by a prefix matcher but do not have a specific mapping fall back to translation of the raw `message`.

Both fallback progress messages and backend error text accept these formats, in order:

1. `i18n:<key>` translates the trimmed key with no parameters.
2. A JSON string shaped like `{"key":"translation.key","params":{"name":"value"}}` translates the key with the provided parameters.
3. A dotted token containing only letters, digits, underscore, period, and hyphen is translated only when i18next resolves it to a different string.
4. Empty, malformed, unknown, or free-form text is shown unchanged.

Successful action summaries use frontend translation keys directly. Setting a summary through `setSectionMessage` clears stored progress by default. The thumbnail stop request does this too, although a later accepted progress event can repopulate it before the active render returns its final summary.

## Scan roots and rescanning

Choose Folder opens the native directory picker with multiple selection enabled. A returned string becomes a one-item list; an array is deduplicated by exact string equality; null or an empty array is cancellation. The action snapshots existing roots, calls `addScanRoot` sequentially for every selected path, reloads the root list, and derives its added/skipped summary from the before/after lists. Adding roots does not scan them and does not refresh the asset library.

Remove first stores the exact root path and opens an in-app confirmation. Cancel clears only that candidate. Confirm clears the candidate before starting the exclusive operation, then removes the root and performs the success follow-ups shown below. The root is never removed merely by opening or closing the dialog.

Per-root rescan calls `scanFolder(path)`; Rescan all is disabled when the frontend root list is empty and calls `rescanAllRoots()` otherwise. Both listen for scan progress and display either the normal or partial completion summary returned by the API. Discovery, cleanup, and partial-scan semantics are intentionally not repeated here; see [scanning and indexing](scanning-and-indexing.md).

## Thumbnail bulk actions and cancellation

Render all and Retry failed share the scan section and thumbnail progress matcher. Each sets `thumbnailBulkRunning` inside the runner action and clears it in an inner `finally`, so the Stop button exists only while the API call is outstanding. Their final summaries report generated, failed, skipped-failed, processed/total, and whether processing was cancelled.

Stop calls `cancelRenderAllThumbnails` outside the exclusive runner. While that request is outstanding only the Stop button is disabled. An accepted request reports that stopping was requested; a rejected request reports that no bulk render is running; an exception reports a scan-section cancellation error. The request is cooperative: it does not release the runner lock or clear `thumbnailBulkRunning`. The original render call remains responsible for returning, refreshing the library, publishing its final summary, and releasing the lock. Backend scheduling, cancellation checkpoints, and retry identity are covered by [thumbnails](thumbnails.md).

## Import and export UI orchestration

The import/export controller only chooses paths, sequences confirmation and API calls, formats summaries, and triggers frontend refreshes. CSV import and confirmed database restore also enter the shared tag-mutation maintenance barrier before invoking their backend command. The barrier denies new per-asset tag writes, drains already-dispatched writes, and remains held through identity/cache invalidation:

- CSV export opens a save dialog with a `CSV` filter and default `tags-export-YYYY-MM-DD_HH-mm.csv` name.
- CSV import opens a single-file picker with a `CSV` filter. It has no additional in-app confirmation.
- Database export opens a save dialog with a `ZIP` filter and default `media-backup-YYYY-MM-DD_HH-mm.zip` name.
- Database import opens a single-file `ZIP` picker, stores the selected source path, releases the first runner invocation, and waits for an in-app overwrite confirmation. Confirm clears the stored path and starts a second exclusive operation; Cancel clears it without invoking the import.

The timestamp uses local date/time and minute precision. Cancelling a native file dialog is a successful UI cancellation: no API call is made, the relevant section gets a cancellation message, and the runner unlocks normally.

Do not infer archive contents, overwrite atomicity, CSV matching, validation, or rollback guarantees from this controller. Those are documented in [data safety and portability](data-safety-and-portability.md), with the crossing API shapes in the [IPC contract](../architecture/ipc-contract.md).

## Clear library

The danger-zone button opens a named confirmation dialog. No clear command runs until Yes is chosen. Yes closes the confirmation first, starts an exclusive danger operation, listens only for `library-clear*` progress, and enters the shared tag-mutation maintenance barrier before calling `clearLibraryData`. The barrier waits for active tag writes, blocks new ones, and remains held through the direct frontend reset. Because clear can reject after partial commits, that reset also runs on rejection; scan roots and the library are then refreshed best-effort before the runner publishes the error. On success, roots are reloaded and the removed asset/root/thumbnail counts are reported. No closes the dialog without changing the section message.

## Duplicate resolver

Start opens the resolver before beginning an exclusive duplicate scan. The scan does not set global library loading, but it does lock all settings operations and drives duplicate-section progress. Its result replaces the stored groups and asset count. Rescan performs the same operation without changing dialog visibility.

The resolver virtualizes duplicate groups and queues thumbnails only for asset IDs in its current virtual range. Missing previews use a transparent placeholder; a preview load error also falls back to that placeholder. When the dialog is open and either `open` or `groups` changes, its local form is reset:

- rename inputs are seeded from each path's basename;
- staged changes are cleared;
- staged changes are stored by asset ID, so one asset has at most one current rename or delete action.

Generate UUID changes only the input draft and preserves the current extension; Queue rename trims and stages the draft. Editing an already staged rename updates its staged value. Queue delete replaces any staged rename; Unmark delete or Clear action removes the staged action.

Save all is enabled only when the runner is unlocked, at least one change is staged, and validation succeeds. Validation requires positive known asset IDs and portable Windows file names, including control, trailing dot/space, and device-name restrictions. The backend repeats and extends validation across expected paths/versions, indexed and filesystem targets, and newly introduced duplicate names before mutation.

A pure-rename batch runs immediately. If any delete is present, all staged changes—renames and deletes—are captured in `pendingDuplicateDeleteConfirm` and no mutation runs until the second confirmation. Confirm sends one `apply_duplicate_resolution_batch` request containing the scan revision and each expected path/version. The frontend refreshes and rescans for every structured result, then distinguishes committed success (including missing sources), full rollback, and recovery required; recovery feedback retains affected asset IDs and paths.

The controller performs a defensive empty-name check during rename execution, but the complete group/asset validation lives in the resolver UI service. API callers and future alternate views must not assume `onSaveAll` repeats every UI validation.

## Confirmation ordering and close behavior

Remove-root, clear-library, duplicate-delete, and database-import confirmations are `UiModal` dialogs with explicit actions and translated accessible headings. While the global operation lock is true, their buttons are disabled and Escape/backdrop closing is disabled. Each destructive confirmation is cleared before its asynchronous operation begins, so a failed operation leaves an error message rather than reopening the confirmation automatically.

Database import has two non-overlapping runner phases: file selection, then confirmation, then import. Duplicate deletion is the only intentionally nested modal flow: the resolver stays open underneath the later-rendered delete confirmation. Resolve or cancel that confirmation before interacting with the resolver again.

There is no modal-stack manager. `UiModal` instances and `useSettingsView` each attach independent window Escape listeners. When the duplicate resolver, its delete confirmation, and settings view are all open and unlocked, one Escape event can invoke more than one close callback. There is also no focus trap or focus-return coordination. These are current limitations, not an ordering contract to extend.

## Exact success refresh and reset matrix

In this table, `refreshLibrary` means `Promise.all([asset-query refresh, known-tags refresh])`; `refreshScanRoots` means reload and replace the root list. `reset queue` means reset the frontend thumbnail queue/store bookkeeping. Each runner-backed operation also ends its own section loading state, releases the global settings lock, and, unless opted out, clears global library loading in `finally`.

| Operation after API success | Ordered frontend follow-up |
| --- | --- |
| Add selected scan roots | Reload and replace `scanRoots`; derive the added/skipped summary from the pre-add and post-add lists. No asset/tag refresh or thumbnail reset. |
| Remove confirmed scan root | Reset thumbnail queue; reload `scanRoots`; refresh assets/query pages and known tags. The pending root was already cleared before the API call. The thumbnail path map is not cleared. |
| Rescan one root | Refresh assets/query pages and known tags; then reload `scanRoots`. No explicit thumbnail reset. |
| Rescan all roots | Refresh assets/query pages and known tags; then reload `scanRoots`. No explicit thumbnail reset. |
| Render all thumbnails | Refresh assets/query pages and known tags; publish the thumbnail summary. The inner `finally` clears `thumbnailBulkRunning`. No scan-root refresh or explicit thumbnail reset. |
| Retry failed thumbnails | Refresh assets/query pages and known tags; publish the thumbnail summary. The inner `finally` clears `thumbnailBulkRunning`. No scan-root refresh or explicit thumbnail reset. |
| Request thumbnail cancellation | Publish accepted/rejected/error status and clear `cancelThumbnailRunning`. No refresh/reset; the original render later owns its normal success follow-up. |
| Export CSV | Publish row summary only. No refresh/reset. |
| Import CSV | Refresh assets/query pages and known tags; publish import summary. No root refresh or thumbnail reset. |
| Export database bundle | Publish copied-file/thumbnail summary only. No refresh/reset. |
| Import confirmed database bundle | Reset thumbnail queue and replace `thumbs` with `{}`; reload `scanRoots`; refresh assets/query pages and known tags; publish restore summary. The pending source was already cleared before the API call. |
| Clear library | Reset thumbnail queue; replace `thumbs` with `{}`, assets with `[]`, total with `0`, offset with `0`, and known tags with `[]`; reload `scanRoots`; publish clear summary. It does not call `refreshLibrary`. |
| Find/rescan duplicates | Replace duplicate groups and count; publish scan summary. No library/root refresh or thumbnail reset, although visible duplicate IDs may be queued for previews. |
| Apply duplicate changes | After the backend batch returns, refresh assets/query pages and known tags; scan duplicates; replace groups/count/revision; publish committed, rolled-back, or recovery-required summary. No root refresh or explicit thumbnail reset. |

Native picker cancellation, confirmation cancellation, an empty duplicate change list, and a rejected/no-op thumbnail stop request perform no refresh or lifecycle reset. On failure, the runner does not perform compensating refreshes beyond work already completed inside the action before the error. Confirmed database restore and clear-library are deliberate exceptions: either command may reject after changing live state, so their identity-bound frontend caches are invalidated inside the tag-mutation barrier and their root/library refreshes are attempted best-effort before the original error is shown. Other follow-ups remain sequential: if a refresh itself fails, later follow-ups and the intended success summary do not run.

## Current guarantees and known limitations

Current guarantees:

- The ref lock prevents two runner-backed settings operations from starting through one mounted frontend controller, including same-tick calls before disabled controls rerender.
- Only the initiating section shows loading/progress, while the global lock disables normal actions in every operation section.
- Progress listeners are phase-filtered and unregistered in the runner's `finally` path.
- Native-dialog cancellation and explicit confirmation cancellation do not invoke the destructive/import API.
- Confirmation targets are captured before the dialog closes, so the confirmed root, archive path, or duplicate change set is the one passed to the action.
- Root removal, database restore, clear-library, scans, imports, thumbnail renders, and duplicate application use the exact refresh/reset ordering in the matrix; restore and clear additionally invalidate and best-effort refresh after an uncertain command rejection.
- Settings operations and their feedback continue while the full-page settings view is closed because their controller remains mounted in the shell.

Known limitations:

- The exclusive runner is not generally a backend safety boundary and does not coordinate another window, direct IPC caller, or a second frontend instance. Bundle export/restore independently use the backend's process-wide database maintenance gate.
- The runner does not expose cancellation for scans, imports, clear-library, duplicate scans, or duplicate application. Only bulk thumbnail rendering has a stop request.
- A second runner call made while locked returns silently; it does not queue work or publish a message.
- The progress listener has no operation identifier beyond phase filtering. Correct attribution depends on exclusivity plus disjoint phase families.
- The runner unregisters its listener when the action settles, but it has no separate unmount abort/stale-result guard if the entire shell is destroyed mid-operation.
- Duplicate rename targets cannot be another source in the same batch, so swaps/cycles require independent temporary names in separate rescanned batches.
- The DB transaction does not make post-commit staged-delete or thumbnail cleanup transactional; those cases are surfaced as recovery-required results.
- Root addition is sequential. A failure after one addition can leave earlier roots added while preventing the final root-list refresh.
- Local lifecycle resets intentionally differ by operation; notably root removal does not clear the thumbnail path map, and duplicate mutations do not explicitly reset it.
- Nested modal Escape/focus behavior is not coordinated, as described above.

## Change checklist

When changing settings behavior:

1. Keep raw `invoke` and event serialization in `src/api.ts`; update the [IPC contract](../architecture/ipc-contract.md) for boundary changes.
2. Decide whether the action belongs to an existing section and whether it must use the shared exclusive runner. Document any intentional exception like thumbnail cancellation.
3. Choose `setGlobalLoading` deliberately. Do not confuse the global settings lock with the per-section loading/progress state or library gallery loading.
4. If the backend emits progress, add a non-overlapping matcher, translated known phases, raw-message fallback coverage, and guaranteed listener cleanup.
5. Define native-picker cancellation, in-app confirmation, locked close behavior, and the point at which pending confirmation state is cleared.
6. For destructive/import work, keep frontend orchestration here and update [data safety and portability](data-safety-and-portability.md) for backend atomicity, rollback, archive, and filesystem guarantees.
7. Update every affected cache deliberately: assets/query pages, known tags, scan roots, thumbnail queue/store/map, duplicate results, and selection state. Amend the refresh/reset matrix when behavior changes.
8. For duplicate actions, preserve staged-action validation, delete confirmation for mixed batches, deterministic execution order, and post-success duplicate rescan; add failure coverage if transactional behavior changes.
9. Preserve localized visible messages, dialog names, disabled states, and progress accessibility. Test Escape/backdrop behavior when adding another nested layer.
10. Run focused service/hook/component tests, the relevant `App.test.tsx` integration cases, and a build for contract or composition changes.

## Test map

- `src/hooks/__tests__/useSettingsActions.test.ts` covers root add/remove/rescan, progress updates, cross-section exclusion, thumbnail summaries/cancel errors, clear-library, picker cancellation, duplicate scan/basic validation, CSV import, and bundle export.
- `src/hooks/__tests__/useSettingsActionsConfirmFlow.test.ts` covers cancel/confirm ordering for mixed duplicate deletion and database-bundle overwrite.
- `src/components/settings/services/__tests__/operationStateService.test.ts`, `progressService.test.ts`, and `duplicateValidationService.test.ts` cover ready state, progress formatting/matchers, thumbnail summaries, file-name validation, case-insensitive collisions, unknown assets, and touched-group rules.
- `src/components/settings/hooks/__tests__/useDuplicateResolverHandlers.test.ts` covers rename draft updates and queue/delete toggling.
- `src/components/settings/__tests__/*` covers panel wiring, operation-status rendering, destructive-dialog controls, and resolver staging/locking; `src/components/settings/sections/__tests__/*` covers section callbacks and locked disabled states.
- `src/__tests__/App.test.tsx` covers the full-page settings route, Back/Escape, highlighted first-folder route, root confirmation, thumbnail stop/retry, section loaders, timestamped export names, and disabling other settings actions during a running operation.

The current tests do not comprehensively exercise listener-registration failure, translated JSON/backend error formats, nested Escape competition, partial duplicate-application failure, or every cell of the refresh/reset matrix. Treat those as gaps when changing the corresponding behavior.
