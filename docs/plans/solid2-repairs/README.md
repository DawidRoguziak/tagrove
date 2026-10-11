# Solid 2 review repairs

This is the repair handoff from the [completed review](../solid2-deep-review/PROGRESS.md). Planning is complete; implementation has not started. [PROGRESS.md](PROGRESS.md) records the current repair batch and its verification.

The review confirmed fourteen P2 and six P3 findings. No P0/P1 findings or unresolved candidates remain. This plan assigns every confirmed finding to one batch. DR-017 and DR-021 were rejected and are excluded from repairs.

The [final report](../../../raports/20261010T182819Z-solid2-deep-review/report.md), [catalogue](../../../raports/20261010T182819Z-solid2-deep-review/finding-catalogue.md) and linked independent verdicts are local, ignored evidence. The descriptions and acceptance checks below preserve the repair scope for checkouts without those artifacts. They describe intended behavior, not fixes already made.

## Baseline and execution

Planning started at `6fd951d2e7fd38e643223cd8c7582e2693e6fd66` on 2026-10-11 with a clean working tree. The review baseline is `ad277bd53b23201af297bccc1e454272d8f3d349`. The prescribed final validator passed before planning, including 693 verifier hash entries across 224 files and 109 preserved original artifacts. These checks establish evidence applicability; they are not a fresh application test run.

Use the current checkout. Never create a worktree. Do not commit or push without an explicit request. Keep synthetic fixtures, raw logs and screenshots in ignored local storage, and preserve app-data/profile and cleanup guards.

At each batch start, inspect current changes and the owning source, tests and documentation. Read the linked contract sections before changing a boundary. Confirm that the recorded trigger still applies, then add a regression that distinguishes broken from corrected behavior. Keep unrelated edits intact. Update the owning documentation with any changed contract and record exact verification results and remaining limits in progress.

Keep the completed review immutable as historical evidence. Once repairs change source, its original hash validator is expected to report drift. Record that drift and new regression evidence in the repair record; do not replace historical verifier hashes to make validation pass.

## Repair order

The order is an implementation choice based on shared ownership and verification dependencies. It does not change review priorities or imply measured frequency within P2. Batch 01 repairs a false-pass test before startup changes. Batch 09 corrects measurement provenance before batch 10 changes scrolling. Batch 05 shares favorite and metadata ownership, but each finding still needs its own regression and disposition.

| Batch | Confirmed findings | Work | Prerequisites |
| --- | --- | --- | --- |
| 01 | DR-001, DR-002 | Reliable startup-disposal regression and accurate test guide | None |
| 02 | DR-020 | Boot with unavailable preference storage | None |
| 03 | DR-003 | Startup-scan completion across frontend remount | 01 |
| 04 | DR-004, DR-005 | Query admission order and explicit capacity failure | None |
| 05 | DR-006, DR-013, DR-015 | Details invalidation and favorite operation ownership | 04 before integrated query-failure checks |
| 06 | DR-010, DR-011 | Suggestion recovery and literal tag-name selection | None |
| 07 | DR-012, DR-014 | Lightbox page retry and current-image load ownership | 04, 05 before integrated navigation checks |
| 08 | DR-008 | Preserve failed range-selection retry after layout changes | 04 |
| 09 | DR-022 | Correct gallery timing provenance | Before performance comparisons in 10 |
| 10 | DR-007, DR-009 | Bounded physical scrolling and accurate gallery contract | 08, 09 |
| 11 | DR-016 | Native-control Escape routing | None; serialize desktop access |
| 12 | DR-018 | Reject stale thumbnail Stop publication | None |
| 13 | DR-019 | Translate remaining tag-list and search-error messages | 06 before combined locale interaction checks |
| 14 | None | Combined regression and completion record | 01 through 13 |

## 01 Reliable test foundation

[DR-002 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-002.json), P3. `src/app/__tests__/runtime.test.ts` currently checks the empty startup snapshot before the async adapter consumer and Solid publication finish. Removing the production disposal guard still passes the test.

Strengthen this test to await actual consumer completion and then the publication boundary. Preserve production disposal behavior and the real `src/platform/tauri/tags.ts` adapter. Do not use an already-true empty-snapshot `waitFor`, arbitrary sleep or a guessed count of microtasks as proof of completion. Prefer test-local instrumentation over changing the runtime API solely for a test. Read `src/app/createAppRuntime.ts`, `src/test/settle.ts` and the original verifier's completion probe before choosing the mechanism.

Acceptance: unchanged production passes; removing only the success disposal guard in an isolated in-memory transform makes the strengthened test fail with the late value. Check that the transform matches exactly once and does not alter on-disk application source. Keep a guard-preserving control and the existing startup/idempotence/failure cases. Discover the focused test files before running a custom probe config so merged include arrays cannot silently expand the run.

[DR-001 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-001.json), P3. Correct the test guide's claim that the full backend script runs two integration binaries. Describe all Cargo-discovered integration targets and distinguish default tests from ignored diagnostics. Compare `package.json`, the backend manifest and locked offline Cargo metadata; avoid replacing one fixed count with another.

Read and update [testing](../../development/testing.md). Read [frontend scheduling and cleanup](../../frontend/architecture-and-ui-conventions.md#reactivity-and-cleanup) and [runtime ownership](../../architecture/system-overview.md#frontend-owner-lifetimes). The first implementation command is `bun run test -- src/app/__tests__/runtime.test.ts`; passing it alone does not close DR-002 without the mutation check.

## 02 Preference failures must not prevent boot

[DR-020 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-020.json), conditional P2. Storage reads/writes during i18n module evaluation and theme initialization can throw before App mounts.

Own the change in `src/i18n/core.ts`, `src/ui/dom/theme.ts` and their bootstrap callers. Treat preference persistence as fallible at its boundary. Fall back to browser language/English and dark theme when reads fail; apply preferences in memory and to the DOM even if writes fail. Keep ordinary persistence working.

Acceptance: import the real i18n core and run the production mount path under storage-property denial, `getItem` failure, `setItem` failure and quota failure. App mounts with usable preferences in each case; normal storage still persists them. Use isolated module/process state where singleton imports would hide the failure. Native incidence was not measured by the review and must not be presented as established.

Read and update [bootstrap and theme ownership](../../frontend/architecture-and-ui-conventions.md) and [localization](../../development/localization.md).

## 03 Startup scan survives frontend remount

[DR-003 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-003.json), P2. Remount during an active startup scan leaves the replacement runtime without the completion refresh, including when its query is empty or fully cached.

Trace `src/app/createAppRuntime.ts`, `src/features/settings/createScanSettings.ts`, the scanning adapter, `src-tauri/src/commands/scan.rs` and process-owned startup state. Let a replacement runtime observe or await an already-running scan. Preserve once-only launch snapshot consumption, including failure, and distinguish running work from no startup work. Do not depend only on an event that can arrive before subscription.

Acceptance: defer scanning, remount with zero and partially cached rows, complete scanning and observe a replacement query refresh without a second scan. Cover remount after completion, scan failure and disposal. Add native state/command tests and a real desktop remount-during-scan check with synthetic media for the changed boundary.

Read and update [system lifecycle](../../architecture/system-overview.md), [scanning](../../subsystems/scanning-and-indexing.md), [settings operations](../../subsystems/settings-operations.md) and [scan IPC](../../architecture/ipc-contract.md#scan-roots-and-scanning).

## 04 Query admission and capacity outcomes

[DR-004 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-004.json), P2. Async command polling can register an older frontend request last. Both responses then leave the current frontend without its intended query session.

Change admission ownership in `src-tauri/src/commands/assets.rs` and `src-tauri/src/services/asset_query_service.rs` with the frontend query adapter/model. Choose synchronous admission before spawned work, or explicit owner identity with monotonic per-owner generations. A process-global maximum frontend generation is invalid because remount restarts generations. Define a recoverable outcome if the current frontend request is superseded.

Acceptance: invert native admission order and both frontend reply orders; the newest filter receives a usable session or visible actionable error. A new frontend owner can query after the prior owner's higher generation. Preserve genuine supersession and cache invalidation behavior.

[DR-005 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-005.json), P2. An authoritative query exceeding the retained session budget is silently reported as superseded. Give capacity rejection an explicit error outcome without removing the memory budget or retrying indefinitely.

Acceptance: use a tiny test-local budget with an unchanged latest token and cache epoch. Rejection must expose a query error in empty and previously populated views; narrowing to a fitting query recovers. Below-budget native queries still page and resolve positions. Do not require millions of database rows to test this boundary.

Read and update [query/gallery](../../subsystems/library-query-and-gallery.md), [database](../../subsystems/database.md), [query IPC](../../architecture/ipc-contract.md#query-session-states) and its [contract-change checklist](../../architecture/ipc-contract.md#safe-contract-change-checklist). Run native service tests, frontend response tests and a real desktop query path if IPC changes.

## 05 Metadata and favorite ownership

[DR-006 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-006.json), P2. A delayed save can reinsert captured full details after Settings has cleared them following rename/rescan. Guard full-record restoration with the details-cache generation, or leave invalidated details uncached. Continue publishing canonical metadata separately.

Acceptance: hold tag-save completion across Settings rename/rescan and query refresh. Reopening fetches the current path/size and retains saved tags. Normal saves preserve valid details; identity reset and eviction still reject obsolete publication.

[DR-013 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-013.json), P2. Successful favorite removal followed by failed query refresh displays a false save failure. Separate persisted mutation success from refresh failure and expose query/navigation recovery without reversing the favorite.

Acceptance: commit removal under Favorites-only, reject the follow-up refresh, observe saved favorite state and query-specific recovery. Retrying the query must not issue an inverse favorite write. Genuine write failure still reports save failure.

[DR-015 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-015.json), P2. Pending favorite ownership leaks across identity reset and can leave a reused ID disabled. Scope operations and presentation pending state to the identity epoch; obsolete completion cannot publish into or clear a new operation for that ID.

Acceptance: hold an operation across close, identity-resetting maintenance and reopen of the same ID. Old completion leaves the current control usable unless a current-epoch write owns it. Test new-epoch work still pending when the old operation settles. Recheck this claim after DR-013 changes the awaited refresh lifetime; removal of its original trigger is not a substitute for checking epoch ownership.

Primary owners are `src/library/metadata/createMutations.ts`, `createDetailsCache.ts` and `src/features/lightbox/createLightbox.ts`. Read and update [query/cache ownership](../../subsystems/library-query-and-gallery.md), [lightbox](../../subsystems/lightbox.md), [settings](../../subsystems/settings-operations.md) and [maintenance safety](../../subsystems/data-safety-and-portability.md). Use deferred frontend integration checks; use temporary files/databases for any real rename/import verification.

## 06 Search and tag interaction

[DR-010 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-010.json), P2. Suggestion Retry disappears on input blur before the pointer click reaches it. Coordinate `SearchTagator.tsx`, shared tagging UI and `suggestions/createSuggestions.ts` so recovery remains available during focus transfer. Removing the JSX focus condition alone leaves the model clearing failure on ineligibility.

Acceptance: inject worker failure, click Retry with native pointer input, and recover an actual rendered suggestion. Repeat with keyboard focus transfer in search, lightbox and bulk editors. Keep ordinary submission, focus restoration and disposal correct.

[DR-011 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-011.json), P2. Selection dictionaries treat inherited values for `constructor` and `__proto__` as selected tags. Use a Map or consistently safe own-property storage throughout `tag-list/tagListSelectionService.ts` and its callers.

Acceptance: rendered modal single-click/Apply, double-click exclusion and keyboard exclusion all produce literal filters for both names without exceptions. Normal tag selection and deselection remain correct.

Read and update [search/tag contracts](../../subsystems/search-tags-and-media-groups.md) and [frontend focus/layer ownership](../../frontend/architecture-and-ui-conventions.md). Pointer focus ordering requires a native WebView check in addition to component tests.

## 07 Lightbox recovery and image ownership

[DR-012 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-012.json), P2. A failed adjacent page makes later navigation fail locally without a visible retry. Expose navigation failure and an explicit recovery path through `createLightbox.ts` and the query model. Clear/retry the failed target page without introducing unbounded gallery retries.

Acceptance: fail prefetch or Next across the 128-item boundary, restore the backend, use visible retry, and observe a fresh request and successful navigation. Cached Previous works; obsolete retry cannot change a new selection.

[DR-014 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-014.json), P2. Detached image A can deliver load after B mounts, replacing the controls' target and dimensions. In `image/createImageControls.ts`, accept load only for the current image and active owner; retain source/selection identity where needed.

Acceptance: late A load leaves B's dimensions, fit, pan and zoom correct. Include disposed-owner delivery and normal B load. Use a controlled delayed-image native check to exercise DOM delivery as well as model tests.

Read and update [lightbox](../../subsystems/lightbox.md), [query failures](../../subsystems/library-query-and-gallery.md) and [frontend owner cleanup](../../frontend/architecture-and-ui-conventions.md#reactivity-and-cleanup).

## 08 Selection Retry survives error layout

[DR-008 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-008.json), P2. Error-banner geometry causes `createGridHandlers.ts` to unregister listeners and cancel a settled failed selection, erasing its retry intent.

Cancel live gesture work on geometry changes without cancelling a settled failed selection merely because listeners re-register. Preserve explicit cancellation on new input, query/identity changes and disabled selection.

Acceptance: fail an evicted offscreen page needed by Shift/range selection, render the real error banner and deliver its ResizeObserver geometry change. Clicking Retry completes the original selection, with existing selected IDs intact. Cover rectangle failure and genuine live-gesture cancellation.

Read and update [gallery selection](../../subsystems/library-query-and-gallery.md) and [bulk-selection semantics](../../subsystems/search-tags-and-media-groups.md). Use an integrated component test and native banner/layout verification.

## 09 Honest gallery timing

[DR-022 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-022.json), P3. The full `e2e/specs/gallery.performance.e2e.js` suite traverses thumbnails before labeling a later route cold.

Measure first-use sampling before the metadata traversal with the same media filter for later warm passes, exclude the initial loaded viewport explicitly, and record raw samples and cache preparation. If retaining existing order, label the passes as revisits and provide a separate first-use measurement for batch 10. Do not claim an OS-cold cache without establishing it.

Acceptance: execute the entire opt-in suite in declared order, record what was generated before sampling, and verify output labels against that state. Running only the timing test does not exercise this finding. The review established incorrect provenance, not a measured production slowdown.

Read and update [performance verification](../../development/testing.md), [gallery](../../subsystems/library-query-and-gallery.md) and [thumbnail reuse](../../subsystems/thumbnails.md).

## 10 Reach every logical gallery row

[DR-007 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-007.json), P2. A million-result, five-column gallery exceeds WebKit's physical layout height and makes its final rows unreachable. Bound the physical spacer and map scroll coordinates to logical indices in `GalleryGrid.tsx` and its geometry/virtualizer owners.

Acceptance: native WebKit reaches and selects the last logical result at one million rows, across narrow columns, tile-size changes and resize anchors. Check scrollbar dragging, wheel navigation, range/rectangle selection, page demand, bounded mounted tiles and cleanup. Lazy synthetic summaries can prove layout scale; label them separately from live SQLite scale. Use batch 09 measurements for any performance comparison.

[DR-009 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-009.json), P3. Correct the owning guide's obsolete `hasMore=false` and `assetCount` fallback claims. Current production uses `query.offset < query.total` and required `resultCount`; sparse viewport demand is primary, near-end loading remains active, and footer behavior depends on `hasMore` and generation. Describe the final implementation after this batch and verify both footer branches against callers.

Read and update [gallery ownership](../../subsystems/library-query-and-gallery.md), [selection semantics](../../subsystems/search-tags-and-media-groups.md) and [testing](../../development/testing.md). Do not close the layout finding on jsdom measurements alone.

## 11 Native-control Escape

[DR-016 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-016.json), P2. Windowed Escape after native play/volume focus never reaches lightbox close.

Route a session-scoped intent from `src-tauri/src/video_surface.rs` through the native-video adapter/model to the frontend layer owner. Preserve speed-menu handling, fullscreen exit and higher-priority drawer/delete-confirmation behavior. Reject retired-session input.

Acceptance: on the isolated native display, focus play and volume controls and press Escape. Windowed preview closes at the proper layer; menu/fullscreen and nested-overlay controls retain their priority. Include WebView-focus and retired-session countercases. DOM-dispatched keyboard events cannot establish GTK focus routing.

Read and update [native playback](../../subsystems/lightbox.md), [frontend layer ownership](../../frontend/architecture-and-ui-conventions.md) and [video IPC](../../architecture/ipc-contract.md). Run native input verification plus affected Rust/frontend lifecycle tests.

## 12 Stale Stop replies

[DR-018 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-018.json), P3. A late thumbnail Stop reply overwrites final render failure or a newer scan's progress in `createScanSettings.ts`.

Associate render and Stop publication with synchronous operation ownership. Both Stop success and rejection may publish only while that render still owns the section. Clear pending state without clearing a newer operation's ownership.

Acceptance: defer Stop success and rejection across render success/failure and a subsequent scan. Terminal results and the new scan's progress remain authoritative; active-render Stop still works and controls recover.

Read and update [settings runner](../../subsystems/settings-operations.md), [thumbnail cancellation](../../subsystems/thumbnails.md) and [progress IPC](../../architecture/ipc-contract.md#broadcast-progress). Controlled response-order tests are required; a normal Stop smoke alone cannot reproduce the defect.

## 13 Complete affected translations

[DR-019 verdict](../../../raports/20261010T182819Z-solid2-deep-review/verification/DR-019.json), P3. Thirteen tag-list interaction strings and four search errors remain English in each of `cs`, `de`, `es`, `fr`, `it`, `ja`, `ko`, `ru` and `zh`.

Translate those 153 resource values while preserving placeholder identifiers and search grammar tokens. Avoid a blanket English-equality rule because some valid translations and tokens legitimately match English.

Acceptance: `bun run locale:check` and `bun run test:locale-tools` pass. Render included/excluded summaries and accessible tag states, and generate all four validation errors for the nine affected locales. Check English/Polish controls. Record linguistic review limits separately from structural checks; rendered accessible labels do not prove actual speech output.

Read and update [localization](../../development/localization.md) and [search/tag behavior](../../subsystems/search-tags-and-media-groups.md).

## 14 Combined verification and closure

Every confirmed finding needs its own completed regression, changed-source references and remaining limits in progress. A shared implementation does not merge independent findings. Keep the rejected claims excluded.

Use the [test layer matrix](../../development/testing.md#test-layer-matrix). Run focused checks per batch, then run `bun run typecheck`, `bun run lint`, `bun run test`, `bun run test:build`, `bun run locale:check`, `bun run test:locale-tools`, `bun run rust:fmt`, `bun run rust:clippy` and `bun run test:backend` on the combined repair state. Execute the affected desktop paths above with their opt-in flags; a default suite can skip them. Do not add overlapping test counts into a claimed unique total.

Serialize shared builds and desktop sessions. Follow the repository desktop-control skill before native work, use disposable profiles and synthetic media, inspect actual results, and record cleanup. Run ordinary desktop development through `bun run tauri:dev`.

Completion requires all twenty confirmed findings to pass their acceptance checks, updated owning docs and a clean diff check. Record any unexecuted required check as pending or blocked with its reason. Keep broader unverified environments, such as Wayland or release packaging, explicit rather than implying those were tested. No dependency upgrade or unrelated redesign is part of these repairs.
