# Settings operations

Implementation entry points: [settings composition](../../src/features/settings/createSettings.ts), [operation runner](../../src/features/settings/createSettingsOperationRunner.ts), [import controller](../../src/features/settings/createImportExportSettings.ts), [progress translation](../../src/features/settings/progressService.ts).

This page owns settings state, operation sequencing, progress presentation, confirmations, and cache resets. [IPC](../architecture/ipc-contract.md) owns transport shapes, [scanning](scanning-and-indexing.md) owns indexing, [thumbnails](thumbnails.md) owns rendering/cancellation, and [data safety](data-safety-and-portability.md) owns CSV/bundle validation and recovery.

## Application and view lifetimes

`src/features/settings/createSettings.ts` composes the operation runner and scan,
import/export, duplicate and clear-library models under `createAppRuntime`. Their lifetime
matches the application owner, so closing the settings view preserves operations, progress,
roots and confirmations. `SettingsPanel.tsx` renders all seven sections with the existing
styles and shared modal layers. It takes grouped controllers directly.

The runner uses a synchronous admission lock and reactive section status. It removes
an active progress listener on owner disposal and removes a late registration immediately,
without dispatching its action. Closing only the settings view does not dispose this owner.
Thumbnail cancellation has its own synchronous lock. Confirmed root removal, restore and
mixed duplicate batches consume synchronous pending values before dispatch.

The duplicate resolver uses `virtual-core`, the library's per-asset thumbnail subscriptions,
and one form model. Visible rows carry their group from the same scan snapshot, so an old
virtual index cannot read a removed group during a rescan. Confirmation dialogs retain their
path, counts and disabled state through exit with `createModalSnapshot`.

Rows keep stable identities within a scan snapshot while virtual positions update
reactively, preserving visible editors, focus and thumbnail subscriptions during scrolling.
Measurement registration runs in `onSettled`, after attachment and `data-index`
initialization; rows removed before that callback are skipped. The duplicate dialog
component tests exercise measured spacing and focus/subscription retention with real
`virtual-core` and synthetic DOM dimensions.

The operation ordering below uses `platform/tauri`
wrappers and `library.tags.runWithMutationBarrier` for CSV import, restore and clear.
Restore resets identity, query and thumbnails, retaining vocabulary until refresh; clear
also empties vocabulary immediately. The [IPC](../architecture/ipc-contract.md) and [profile](../architecture/system-overview.md#profiles-identifiers-and-data-isolation) contracts remain backend-owned.

## Settings layout

The full-page view keeps all operation sections mounted. A 240px navigation column links to
scan settings, appearance, language, import/export, duplicates, the danger zone and About in that order. At
widths below 1000px navigation wraps above the content. The page grid stays within the
viewport; the page title truncates when space beside the window controls is tight.
Selecting a link scrolls to and
focuses the section; it does not start an operation, change the URL or persist a new setting.
The first-folder route still highlights and scrolls to scanning. Shared controls, compact
folder rows and dialogs follow the [frontend visual system](../frontend/architecture-and-ui-conventions.md#light-and-dark-visual-system).

## Composition and state ownership

`createAppRuntime` constructs `createSettings` even when the gallery is visible. The settings action state therefore has shell lifetime: it survives switching between the gallery and the lazy-loaded `AppSettingsView`, but not a complete app unmount or restart. On shell mount, scan roots are hydrated alongside known tags. The runtime view signal owns whether the full-page settings view is open; Back and the shell's registered settings-layer Escape handler return to the gallery.

`AppSettingsView` supplies the sticky page header and renders `SettingsPanel`. `SettingsPanel` is the rendering/composition boundary for seven sections:

1. `ScanSettingsSection` lists roots and exposes add, remove, per-root/all-root rescan, bulk thumbnail render, retry, and stop controls.
2. `AppearanceSection` changes the theme, preserving the light/dark preview sizes.
3. `LanguageSection` displays the language selector directly below Appearance. Its heading and description precede the selector, whose width is capped at 480px. Both preference sections receive values and callbacks from the existing `appearance` controller and bypass the operation runner.
4. `ImportExportSection` exposes CSV and database-bundle import/export.
5. `DuplicatesSection` starts the duplicate scan and shows the last non-zero result.
6. `DangerZoneSection` opens the clear-library confirmation.
7. `AboutSection` displays version, legal text and repository/contact links.

The panel also mounts the remove-root, clear-library, duplicate resolver, duplicate-delete, and database-import confirmation dialogs. Production composition passes the typed `appearance`, `scan`, `importExport`, `duplicates`, and `dangerZone` controller groups. The panel does not own the operation models.

`createSettings` composes one shared `createSettingsOperationRunner` with four focused models:

| Model | Owned state and responsibility |
| --- | --- |
| `createScanSettings` | Roots, video-tool availability, thumbnail bulk/cancel flags, remove-root candidate, and scan/thumbnail actions. |
| `createImportExportSettings` | File dialogs, bundle inspection/root mapping, and the source/mappings awaiting overwrite confirmation. |
| `createDuplicateSettings` | Resolver visibility, pending delete confirmation, duplicate snapshot/revision, and one backend batch application. |
| `createDangerZoneSettings` | Clear-library confirmation and action. |

Duplicate previews use the library's per-asset thumbnail store and queue. `createAppRuntime` supplies lifecycle callbacks from `createLibrary`; settings does not maintain a second asset cache.

## Exclusive operation runner

The runner serializes settings work within the mounted frontend. A synchronous `operationLock` rejects a second call immediately, before Solid publishes the reactive lock state. Each model exposes the shared lock to its section, so their normal action buttons are disabled together. This is a UI-level lock only; backend concurrency and workflow locks are described in the [system overview](../architecture/system-overview.md).

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

1. Return without doing anything if the synchronous lock is already set.
2. Set the synchronous lock and rendered global lock.
3. Unless `setGlobalLoading: false` was requested, set the library browser's global loading state to true.
4. Mark the initiating section loading, set its pending message, and clear progress.
5. If configured, await registration of a `process-progress` listener.
6. Await the supplied action.
7. On any listener/action/follow-up error, replace that section's message with `<translated error prefix>: <translated backend text>` and clear progress. Errors are presented, not rethrown by the runner.
8. In `finally`, unregister the progress listener, clear that section's loading flag, clear global loading when it was set, and release both locks. The final message remains visible.

Folder selection, startup-preference saves, duplicate scans and duplicate change application set `setGlobalLoading: false`; the other runner-backed actions use global loading. Duplicate operations still acquire the shared settings lock and show duplicate-section loading. Thumbnail cancellation deliberately bypasses `runExclusiveOperation`: it must remain callable while the bulk thumbnail operation owns the lock. A synchronous pending token prevents duplicate Stop requests for the active render; `cancelThumbnailRunning` reflects that token in the disabled Stop button.

Closing the settings page does not cancel an operation because the settings models remain owned by the application. Reopening the page exposes the same lock, messages, progress, roots, and duplicate results.

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

Successful action summaries use frontend translation keys directly. Setting a summary through `setSectionMessage` clears stored progress by default. An active render's Stop response does this too, although a later accepted progress event can repopulate it before the render returns its final summary. Retired Stop responses leave both message and progress unchanged.

## Scan roots and rescanning

The scanned-folder list initially shows the first three roots in controller order. With more
than three roots, a centered Show more button reveals the full list and changes to Show less.
Expansion belongs to `ScanSettingsSection`: root refreshes preserve it, while closing and
reopening Settings resets it. With three or fewer roots, all rows appear without a toggle.
The toggle remains usable during operations and exposes `aria-expanded` and `aria-controls`.
Collapsing only changes visible rows; per-root actions and full-library operations keep their
existing scope.

Choose Folder opens the native directory picker with multiple selection enabled. A returned string becomes a one-item list; an array is deduplicated by exact string equality; null or an empty array is cancellation. The action snapshots existing roots, calls `addScanRoot` sequentially for every selected path, reloads the root list, and derives its added/skipped summary from the before/after lists. Adding roots does not scan them and does not refresh the asset library.

Remove first stores the exact root path and opens an in-app confirmation. Cancel clears only that candidate. Confirm clears the candidate before starting the exclusive operation, then removes the root and performs the success follow-ups shown below. The root is never removed merely by opening or closing the dialog.

Per-root rescan calls `scanFolder(path)`; Rescan all is disabled when the frontend root list is empty and calls `rescanAllRoots()` otherwise. Both listen for scan progress and display either the normal or partial completion summary returned by the API. Discovery, cleanup, and partial-scan semantics are intentionally not repeated here; see [scanning and indexing](scanning-and-indexing.md).

`createScanSettings` reads `getVideoToolStatus` once on mount. Pending status is null; a rejected check becomes both tools unavailable. The section warns when ffmpeg or ffprobe is unavailable. This does not measure libmpv playback capability and does not automatically recheck after installing tools.

Each folder row includes a localized, accessible **Scan on app startup** checkbox. Its saved value is false for existing and newly added roots. Saving uses the shared runner with `setGlobalLoading: false`, updates the row after backend success, and keeps the previous value on error. The checkbox is disabled while a settings operation holds the lock. Enabling it takes effect at the next app launch.

The shell invokes `scanOnStartup` at mount, independently of whether Settings is open. Synchronous once-only flags in the runtime and scan model prevent repeated dispatch; the backend process owns the authoritative once-only snapshot and retains its result. A replacement runtime joins an active startup scan or reads its terminal result. The startup action uses the scan progress matcher and ordinary loading/summary behavior. A null response means no roots were enabled at launch and clears the pending message without refreshing the library. A scan result refreshes assets, tags, and roots; rejection attempts both refreshes before reporting the error because scan batches may already have committed. Disposed scan models skip these completion follow-ups. No automatic retry runs during that process.

## Thumbnail bulk actions and cancellation

Render all and Retry failed share the scan section and thumbnail progress matcher. Each assigns a synchronous render token and sets `thumbnailBulkRunning` inside the runner action. Its inner `finally` retires the token and clears both bulk and Stop-pending state. The Stop button exists through the render call and its library refresh. Their final summaries report generated, failed, skipped-failed, processed/total, and whether processing was cancelled.

Stop calls `cancelRenderAllThumbnails` outside the exclusive runner, only while a render token is active. While that render's Stop request is outstanding only the Stop button is disabled. A `true` reply reports that stopping was requested; `false` reports that no bulk render is running; an exception reports a scan-section cancellation error. All three may publish only while the captured render still owns the section and the model is mounted. Replies after render completion cannot overwrite its summary/error or a subsequent scan's progress. A new render may send Stop even if the previous render's reply is still pending; the old request's cleanup cannot clear the new request's pending token. The request is cooperative: it does not release the runner lock or clear `thumbnailBulkRunning`. The original render call remains responsible for returning, refreshing the library, publishing its final summary, and releasing the lock. Backend scheduling, cancellation checkpoints, and retry identity are covered by [thumbnails](thumbnails.md).

Controlled response-order regressions in [`thumbnailStop.test.tsx`](../../src/features/settings/__tests__/thumbnailStop.test.tsx) cover both render modes, terminal success/failure, subsequent scan progress, newer pending Stop ownership, disposal and rendered controls. They mock adapter replies and do not measure native delivery timing. Run them with `bun run test -- src/features/settings`.

## Import and export UI orchestration

The import/export controller only chooses paths, sequences confirmation and API calls, formats summaries, and triggers frontend refreshes. CSV import and confirmed database restore also enter the shared tag-mutation maintenance barrier before invoking their backend command. The barrier denies new per-asset tag writes, drains already-dispatched writes, and remains held through identity/cache invalidation:

- CSV export opens a save dialog with a `CSV` filter and default `tags-export-YYYY-MM-DD_HH-mm.csv` name.
- CSV import opens a single-file picker with a `CSV` filter. It has no additional in-app confirmation.
- Database export opens a save dialog with a `ZIP` filter and default `media-backup-YYYY-MM-DD_HH-mm.zip` name.
- Database import selects a ZIP and calls `inspectDbBundle`. If mapping is required, it asks for one target directory per source root. Cancelling any picker stops before confirmation. It then stores the source/mappings and releases the runner while awaiting overwrite confirmation; Confirm captures and clears both before starting the import phase.

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

Settings and its dialogs use [the shared UI layer manager](../frontend/architecture-and-ui-conventions.md#modal-and-overlay-conventions). `SettingsView` owns page-level Escape; nested dialogs register above it, so managed dismissal targets the top layer and focus returns to its trigger when available. Keep this registration when adding a confirmation.

## Exact success refresh and reset matrix

In this table, `refreshLibrary` means `Promise.all([asset-query refresh, known-tags refresh])`; `refreshScanRoots` means reload and replace the root list. `reset queue` means reset the frontend thumbnail queue/store bookkeeping. Each runner-backed operation also ends its own section loading state, releases the global settings lock, and, unless opted out, clears global library loading in `finally`.

| Operation after API success | Ordered frontend follow-up |
| --- | --- |
| Add selected scan roots | Reload and replace `scanRoots`; derive the added/skipped summary from the pre-add and post-add lists. No asset/tag refresh or thumbnail reset. |
| Remove confirmed scan root | Reset thumbnail queue; reload `scanRoots`; refresh assets/query pages and known tags. The pending root was already cleared before the API call. Queue reset clears the shared thumbnail store. |
| Save startup-scan preference | Update the saved root flag locally. No library refresh or scan. |
| Startup scan | For a scan result, refresh assets/tags, then roots. On rejection, attempt both refreshes; null requires neither. |
| Rescan one root | Refresh assets/query pages and known tags; then reload `scanRoots`. No explicit thumbnail reset. |
| Rescan all roots | Refresh assets/query pages and known tags; then reload `scanRoots`. No explicit thumbnail reset. |
| Render all thumbnails | Refresh assets/query pages and known tags; publish the thumbnail summary. The inner `finally` clears `thumbnailBulkRunning`. No scan-root refresh or explicit thumbnail reset. |
| Retry failed thumbnails | Refresh assets/query pages and known tags; publish the thumbnail summary. The inner `finally` clears `thumbnailBulkRunning`. No scan-root refresh or explicit thumbnail reset. |
| Request thumbnail cancellation | Publish accepted/rejected/error status and clear `cancelThumbnailRunning`. No refresh/reset; the original render later owns its normal success follow-up. |
| Export CSV | Publish row summary only. No refresh/reset. |
| Import CSV | Invalidate authoritative tags and the shared details cache inside the metadata barrier on success or rejection; after release, refresh assets and known tags, best-effort on rejection. Publish the success summary only after a successful refresh. |
| Export database bundle | Publish copied-file/thumbnail summary only. No refresh/reset. |
| Import confirmed database bundle | Reset metadata identity, query and thumbnail queue/store; reload `scanRoots`; refresh assets/query pages and known tags; publish restore summary. The pending source was already cleared before the API call. |
| Clear library | Reset metadata identity and thumbnail queue/store; replace assets with `[]`, total with `0`, offset with `0`, and known tags with `[]`; reload `scanRoots`; publish clear summary. It does not call `refreshLibrary`. |
| Find/rescan duplicates | Replace duplicate groups and count; publish scan summary. No library/root refresh or thumbnail reset, although visible duplicate IDs may be queued for previews. |
| Apply duplicate changes | After the backend batch returns, refresh assets/query pages and known tags; scan duplicates; replace groups/count/revision; publish committed, rolled-back, or recovery-required summary. No root refresh or explicit thumbnail reset. |

Picker or confirmation cancellation, an empty duplicate batch, and a no-op thumbnail stop perform no lifecycle reset. The runner has no generic compensating refresh. CSV import, database restore, and clear-library actions explicitly invalidate their affected caches inside the metadata barrier on both success and rejection, then attempt relevant refreshes before showing an error. CSV database work is transactional, but an IPC rejection alone does not establish what the frontend observed. Other follow-ups remain sequential; a failed refresh prevents later follow-ups and the intended success summary.

Settings rename/rescan refreshes clear the shared details-cache generation. A metadata
save held across that refresh can still publish canonical metadata, but cannot reinsert
its captured old path or size. Identity-resetting maintenance also retires lightbox favorite
pending ownership, so reusing an asset ID does not inherit an old disabled control.
`runtimeIntegration.test.ts` exercises the actual Settings controllers with deferred IPC;
`createLightbox.test.ts` covers old favorite completion while a new-epoch write is pending.

## Known limitations

- The exclusive runner is not generally a backend safety boundary and does not coordinate another window, direct IPC caller, or a second frontend instance. Bundle export/restore independently use the backend's process-wide database maintenance gate.
- The runner does not expose cancellation for scans, imports, clear-library, duplicate scans, or duplicate application. Only bulk thumbnail rendering has a stop request.
- A second runner call made while locked returns silently; it does not queue work or publish a message.
- The progress listener has no operation identifier beyond phase filtering. Correct attribution depends on exclusivity plus disjoint phase families.
- The runner unregisters its listener when the action settles or its owner disposes and ignores late progress. It does not cancel an already-dispatched backend operation when the application is destroyed.
- Duplicate rename targets cannot be another source in the same batch, so swaps/cycles require independent temporary names in separate rescanned batches.
- The DB transaction does not make post-commit staged-delete or thumbnail cleanup transactional; those cases are surfaced as recovery-required results.
- Root addition is sequential. A failure after one addition can leave earlier roots added while preventing the final root-list refresh.
- Local lifecycle resets intentionally differ by operation. Root removal explicitly resets the thumbnail queue/store; duplicate mutations refresh the query, whose refresh also resets the store.
- Layer-manager focus restoration requires a connected trigger. Native GTK video controls are outside its DOM focus model; see [frontend layer limits](../frontend/architecture-and-ui-conventions.md#keyboard-and-accessibility).

## Change checklist

When changing settings behavior:

1. Keep raw `invoke` and event serialization in `src/platform/tauri`; update the [IPC contract](../architecture/ipc-contract.md) for boundary changes.
2. Decide whether the action belongs to an existing section and whether it must use the shared exclusive runner. Document any intentional exception like thumbnail cancellation.
3. Choose `setGlobalLoading` deliberately. Do not confuse the global settings lock with the per-section loading/progress state or library gallery loading.
4. If the backend emits progress, add a non-overlapping matcher, translated known phases, raw-message fallback coverage, and guaranteed listener cleanup.
5. Define native-picker cancellation, in-app confirmation, locked close behavior, and the point at which pending confirmation state is cleared.
6. For destructive/import work, keep frontend orchestration here and update [data safety and portability](data-safety-and-portability.md) for backend atomicity, rollback, archive, and filesystem guarantees.
7. Update every affected cache deliberately: assets/query pages, known tags, scan roots, thumbnail queue/store/map, duplicate results, and selection state. Amend the refresh/reset matrix when behavior changes.
8. For duplicate actions, preserve staged-action validation, delete confirmation for mixed batches, deterministic execution order, and post-success duplicate rescan; add failure coverage if transactional behavior changes.
9. Preserve localized visible messages, dialog names, disabled states, and progress accessibility. Test Escape/backdrop behavior when adding another nested layer.
10. Run focused helper/model/component tests, the relevant `src/app/__tests__/runtimeIntegration.test.ts` cases, and a build for contract or composition changes.

## Test map

- `src/features/settings/__tests__/createSettingsActions.test.tsx` covers root add/remove/rescan, progress updates, cross-section exclusion, thumbnail summaries/cancel errors, clear-library, picker cancellation, duplicate scan/basic validation, CSV import, and bundle export.
- `src/features/settings/__tests__/createSettingsActionsConfirmFlow.test.tsx` covers cancel/confirm ordering for mixed duplicate deletion and database-bundle overwrite.
- `src/features/settings/__tests__/operationStateService.test.ts`, `progressService.test.ts`, and `duplicateValidationService.test.ts` cover ready state, progress formatting/matchers, thumbnail summaries, file-name validation, case-insensitive collisions, unknown assets, and touched-group rules.
- `src/features/settings/__tests__/createDuplicateResolver.test.tsx` covers rename draft updates and queue/delete toggling.
- `src/features/settings/__tests__` covers panel wiring, operation-status rendering, destructive-dialog controls, and resolver staging/locking; the same directory covers section callbacks and locked disabled states.
- `src/app/__tests__/shell.test.tsx` covers settings navigation, Back/Escape, the first-folder highlight, themes and ordered language choices. `runtimeIntegration.test.ts` composes real settings/library/lightbox/bulk models for CSV draining/invalidation and cleared-ID reuse. Export names and cross-section locks live in the settings model suites.

`operationSafety.test.tsx` and the settings model tests cover listener-registration failure, late registration/disposal, phase filtering, translated JSON/backend errors, synchronous exclusion, cancellation, root mapping, and uncertain maintenance rejection with the actual metadata barrier. Desktop checks cover nested duplicate confirmations and persistence. Exhaustive focus transitions and every backend partial-failure case remain outside this frontend coverage.

## Language card verification

The focused `src/app/__tests__/shell.test.tsx` and `SettingsPanel` tests cover theme
selection, ordered language options, callbacks, and navigation focus. The opt-in desktop
spec checks both cards in English and Polish at 1440, 600, and 320 pixels, including
responsive placement, preview height, immediate switching, and persistence after reload.
Run it on a private X11 display with temporary XDG directories as described in
[desktop testing](../development/testing.md#real-desktop-e2e):

```sh
MEDIATAGGER_SETTINGS_LANGUAGE=1 bun run test:e2e:tauri --spec e2e/specs/settings-language.e2e.js
```

Screenshots stay under ignored `artifacts/settings-language/`.

## About, legal texts and external links

`sections/AboutSection.tsx` is the last settings section, with the `settings-about`
navigation target and the same focus/scroll behavior as other sections. It reads the
publisher name and repository from `packaging/flatpak/publisher.json`, and the build
version and license identifier from `package.json`. The root `LICENSE` and `PRIVACY.md`
are bundled as raw text in native disclosure blocks, available offline in English.
Short UI text is localized in all supported languages.

Source and contact buttons call the official Tauri Opener `openUrl` after a click.
Only the repository URL and its subpages are allowed by the main-window capability;
there is no general URL/file opening permission or added Flatpak network permission.
Automatic plugin interception of HTML links is disabled. A failed launch displays a
localized message and a read-only address field for manual selection and copying. Opening is local component state and does not acquire the settings
operation lock. Component tests cover metadata, exact document text, navigation and
opener success/failure; desktop verification is needed to prove the native handoff.
