# React behavior inventory

Reference source commit: `f11126dfafd40f877ecf5e496abc37d77a435e78`.
Stage 1 started at documentation commit `0e1cc02`. All 84 suites and 699 tests
retain their assertions. Stage 2 moves the API and theme suites without changing their assertions. The table assigns each suite to one Vitest project and a future
owner; it does not claim those owners are implemented. Update individual rows when
porting assertions. Pure helpers may remain shared at their current path until moved.

React render-count and StrictMode assertions stay in the reference suites. Their Solid
replacements must check update isolation and owned disposal before removing the originals.
`SuggestionClient.test.ts` remains in React because it also mounts the vocabulary hook.
Shared tests use DOM mocks without importing the React i18n binding.

| Existing suite | Current project | Future stage and owner | Assertion status |
| --- | --- | --- | --- |
| `src/__tests__/App.test.tsx` | react | 2–8, `app` composition and feature integration | Retained, all assertions |
| `src/platform/tauri/__tests__/api.test.ts` | shared | 2, `platform/tauri` command contracts | Moved, all assertions retained |
| `src/components/UI/__tests__/LazyErrorBoundary.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/app/__tests__/shell.test.tsx` |
| `src/components/UI/__tests__/ModalPresence.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/overlays/__tests__/UiModal.test.tsx`, feature confirmations remain for stage 8 |
| `src/components/UI/__tests__/TagEditor.test.tsx` | react | 5, `features/tagging` | React reference retained; Solid counterpart in `src/features/tagging/__tests__/TagEditor.test.tsx` |
| `src/components/UI/__tests__/ThumbnailImage.test.tsx` | react | 4, `features/gallery` | React reference retained; Solid coverage in `src/ui/controls/__tests__/ThumbnailImage.test.tsx` |
| `src/components/UI/__tests__/UiAlert.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/controls/__tests__/controls.test.tsx` |
| `src/components/UI/__tests__/UiModal.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/overlays/__tests__/UiModal.test.tsx` |
| `src/components/UI/__tests__/UiSegmentedControl.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/controls/__tests__/controls.test.tsx` |
| `src/components/app/__tests__/WindowControls.test.tsx` | react | 2, `app/WindowControls` | React assertions retained; Solid counterpart in `src/app/__tests__/WindowControls.test.tsx` |
| `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts` | react | 4, `features/search` | React reference retained; Solid coverage in `src/features/search/__tests__/createSearch.test.ts` and app gallery integration |
| `src/components/app/hooks/__tests__/useAssetTagState.test.ts` | react | 3, `library/metadata` | React assertions retained; Solid counterpart in `src/library/metadata/__tests__/createAssetTagState.test.ts` |
| `src/components/app/hooks/__tests__/useBulkFavorites.test.ts` | react | 5, `features/bulk` selection and metadata | React reference retained; Solid coverage in `src/features/bulk/__tests__/createBulk.test.ts`, stage-4 selection and stage-3 metadata suites |
| `src/components/app/hooks/__tests__/useBulkSelectionController.test.ts` | react | 5, `features/bulk` selection and metadata | React reference retained; Solid coverage in `src/features/bulk/__tests__/createBulk.test.ts`, stage-4 selection and stage-3 metadata suites |
| `src/components/app/hooks/__tests__/useSelectedSummaries.test.ts` | react | 5, `features/bulk` selection and metadata | React reference retained; Solid coverage in `src/features/bulk/__tests__/createBulk.test.ts`, stage-4 selection and stage-3 metadata suites |
| `src/components/app/services/__tests__/assetMutationService.test.ts` | shared | 3, `library/metadata` | Retained, all assertions |
| `src/components/app/services/__tests__/filterService.test.ts` | shared | 4, `features/search` | Retained, all assertions |
| `src/components/app/services/__tests__/libraryInvalidationService.test.ts` | shared | 3, `library/metadata` | Retained, all assertions |
| `src/ui/dom/__tests__/theme.test.ts` | shared | 2, shared theme persistence | Moved, all assertions retained |
| `src/components/app/services/__tests__/tileSizeService.test.ts` | shared | 4, gallery sizing | Retained, all assertions |
| `src/components/bulk/__tests__/BulkActionsSidebar.test.tsx` | react | 5, `features/bulk` | React reference retained; Solid counterpart in `src/features/bulk/__tests__/BulkActionsSidebar.test.tsx` |
| `src/components/bulk/grouping/services/__tests__/applyBulkMediaGroupAction.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/bulk/grouping/services/__tests__/bulkGroupOrderService.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/bulk/tagging/services/__tests__/applyBulkTagsAction.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/gallery/__tests__/GalleryGrid.test.tsx` | react | 4, `features/gallery` | React reference retained; Solid coverage in real-core/cache integration in `src/features/gallery/__tests__/GalleryGrid.test.tsx` |
| `src/components/gallery/__tests__/GalleryStatusFooter.test.tsx` | react | 4, `features/gallery` | React reference retained; Solid coverage in `src/features/gallery/__tests__/GalleryStatusFooter.test.tsx` |
| `src/components/gallery/hooks/__tests__/useGalleryGridHandlers.test.tsx` | react | 4, `features/gallery` | React reference retained; Solid coverage in `src/features/gallery/__tests__/createGridHandlers.test.tsx` |
| `src/components/gallery/hooks/__tests__/useGalleryVirtualGrid.test.tsx` | react | 4, `features/gallery` | React reference retained; Solid coverage in `src/features/gallery/__tests__/createVirtualGrid.test.tsx` plus real DOM/cache integration |
| `src/components/gallery/services/__tests__/selectionGeometry.test.ts` | shared | 4, `features/gallery` | Retained, all assertions |
| `src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid LightboxModal.test.tsx |
| `src/components/lightbox/__tests__/LightboxMediaStage.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid imageControls.test.tsx and LightboxModal.test.tsx; native-video cases covered by Solid video/LightboxVideo.test.tsx |
| `src/components/lightbox/__tests__/LightboxModal.copy.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid presentation.test.ts and LightboxModal.test.tsx |
| `src/components/lightbox/__tests__/LightboxModal.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid LightboxModal.test.tsx; native-video cases covered by Solid video/LightboxVideo.test.tsx |
| `src/components/lightbox/__tests__/MediaGroupSetter.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid MediaGroupSetter.test.tsx |
| `src/components/lightbox/__tests__/nativeVideoLayout.test.ts` | shared | 7, `features/lightbox/video` | Retained, all assertions |
| `src/components/lightbox/__tests__/useLightboxImageControls.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid imageControls.test.tsx |
| `src/components/lightbox/__tests__/useLightboxTagging.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid LightboxModal.test.tsx |
| `src/components/lightbox/__tests__/useNativeVideoSession.test.tsx` | react | 7, `features/lightbox/video` | React retained; Solid createNativeVideoSession.test.ts ports all nine cases and adds early buffering, disposal and label failure coverage |
| `src/components/lightbox/hooks/__tests__/useLightboxActivity.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid presentation.test.ts |
| `src/components/lightbox/hooks/__tests__/useLightboxKeyboardShortcuts.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid LightboxModal.test.tsx, imageControls.test.tsx and video/LightboxVideo.test.tsx |
| `src/components/lightbox/hooks/__tests__/useLightboxModalHandlers.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid LightboxModal.test.tsx and createLightbox.test.ts |
| `src/components/lightbox/hooks/__tests__/useLightboxSidebar.test.ts` | react | 5–7, `features/lightbox` and shared tagging | React reference retained; Solid presentation.test.ts and LightboxModal.test.tsx |
| `src/components/lightbox/services/__tests__/deleteLightboxAssetAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/saveLightboxMediaGroupAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/saveLightboxTagsAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/toggleLightboxFavoriteAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/search/__tests__/SearchTagator.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in `src/features/search/__tests__/SearchTagator.test.tsx` |
| `src/components/search/__tests__/SearchTagatorWrapper.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in wrapper keyboard/focus tests plus `src/app/__tests__/gallery.test.tsx`; legacy parent-commit callbacks are replaced by explicit model commands |
| `src/components/search/hooks/__tests__/useSearchTagatorHandlers.test.ts` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in Solid search component keyboard/caret/IME tests |
| `src/components/search/services/__tests__/inputService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/services/__tests__/suggestionService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/services/__tests__/tokenService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/worker/__tests__/SuggestionClient.test.ts` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in `src/features/search/suggestions/__tests__/SuggestionClient.test.ts` using the shared client and Solid vocabulary |
| `src/components/search/worker/__tests__/useSuggestions.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in `src/features/search/suggestions/__tests__/createSuggestions.test.ts` |
| `src/components/settings/__tests__/ClearLibraryConfirmDialog.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/__tests__/DuplicateResolverDialog.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/__tests__/RemoveScanRootConfirmDialog.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/__tests__/SectionOperationStatus.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/__tests__/SettingsPanel.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/hooks/__tests__/useDuplicateResolverHandlers.test.ts` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/hooks/__tests__/useStartupScan.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/AboutSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/AppearanceSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/DangerZoneSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/DuplicatesSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/ImportExportSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/LanguageSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/sections/__tests__/ScanSettingsSection.test.tsx` | react | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/services/__tests__/duplicateValidationService.test.ts` | shared | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/services/__tests__/operationStateService.test.ts` | shared | 8, `features/settings` | Retained, all assertions |
| `src/components/settings/services/__tests__/progressService.test.ts` | shared | 8, `features/settings` | Retained, all assertions |
| `src/components/tag-list/__tests__/TagListModal.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in `src/features/search/tag-list/__tests__/TagListModal.test.tsx` and pagination races |
| `src/components/tag-list/__tests__/TagListSearchLauncher.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | React reference retained; Solid coverage in toolbar/modal integration and native literal-tag scenario |
| `src/hooks/__tests__/queryRefreshNavigation.test.tsx` | react | 6, `features/lightbox` selection and navigation | React reference retained; Solid coverage in `src/features/lightbox/__tests__/createLightbox.test.ts` |
| `src/hooks/__tests__/useLibraryAssets.test.tsx` | react | 3, `library/query` | React assertions retained; Solid counterpart in `src/library/query/__tests__/createAssetQuery.test.ts` |
| `src/hooks/__tests__/useLibraryBrowser.test.ts` | react | 3, `library/query` | React assertions retained; Solid counterpart in `src/library/__tests__/createLibrary.test.ts` |
| `src/hooks/__tests__/useLibraryKnownTags.test.ts` | react | 3, `library/vocabulary` | React assertions retained; Solid counterpart in `src/library/vocabulary/__tests__/createKnownTags.test.ts` |
| `src/hooks/__tests__/useSelectionState.test.ts` | react | 6, `features/lightbox` selection and navigation | React reference retained; Solid coverage in `src/features/lightbox/__tests__/createLightbox.test.ts` |
| `src/hooks/__tests__/useSettingsActions.test.ts` | react | 8, `features/settings` | Retained, all assertions |
| `src/hooks/__tests__/useSettingsActionsConfirmFlow.test.ts` | react | 8, `features/settings` | Retained, all assertions |
| `src/hooks/__tests__/useThumbnailQueue.test.ts` | react | 3, `library/thumbnails` | React assertions retained; Solid counterpart in `src/library/thumbnails/__tests__/createThumbnailQueue.test.ts` |
| `src/hooks/services/__tests__/thumbnailStore.test.ts` | shared | 3, `library/thumbnails` | Retained, all assertions |
| `src/utils/__tests__/literalTagSearch.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/utils/__tests__/media.test.ts` | shared | 6, framework-independent media helpers | Retained, all assertions |

Stage 2 adds shell mount/disposal, all 11 live translations, theme persistence/native sync,
settings remounts, lazy loading/failure, optional performance cleanup, stale native-window
reads and controlled-input tests. React StrictMode presence coverage remains in the reference;
Solid tests prove cancellation, rapid reopen, disposal and subscription cleanup instead.

Stage 3 adds query-race, details-cache and mutation integration tests, plus Solid per-asset
thumbnail subscription tests. The browser tests now exercise library composition and its
shared mutation commands. Queue assertions read the real per-asset store instead of the
optional React global mirror. Vocabulary fixtures hydrate through public commands instead
of exported setters. Pure thumbnail-store, mutation-helper and query-impact suites remain
shared and execute once. React selection/draft/navigation coverage remains until stage 6.

Stage 4 adds application startup/search integration, real-core virtual ranges, sparse-slot
replacement, per-asset thumbnail DOM isolation, stable overlapping slots and cache eviction
with backward reload. Gesture tests retain the reference pointer, modifier, cancellation,
layer, edge-scroll and click-suppression assertions. Shared selection tests cover global
ranges, retry, same-batch invalidation and identity reset. Footer tests cover all 11 locales.

The new search suites observe synchronous commands, parser/literal/meta payloads, IME,
keyboard/focus, one worker's coalescing/versioning, stale replies, disposal, pagination
fallback and no-progress retry behavior. Pure parser, suggestion, token and rectangle
geometry suites still execute once in `shared`. Full React suites remain unchanged;
Solid bootstrap expectations now target the implemented gallery instead of the shell text.
Native evidence covers the stage-4 portions listed in PROGRESS; bulk mutation and lightbox
assertions in combined desktop scenarios remain for their respective later stages.


Stage 5 adds 77 Solid cases across three suites. TagEditor and sidebar/modal suites retain
controlled draft, disabled/loading, suggestion, keyboard, stale-completion, focus and
closing-frame assertions. Real virtual-core bounds the thousand-item order dialog.
Bulk model tests cover complete canonical tags, contention, duplicate writes, partial
publication, failed refreshes, identity resets, eviction, missing IDs, favorite toggles,
exact group ordering, and disposal. Hydration tests enforce 256-ID batches and four requests,
and prove that late summaries cannot replace already committed metadata. The shared pure
merge/order helpers and their existing suites still run once.

Desktop tag-editor coverage now includes an independent bulk persistence/vocabulary scenario
that runs before the lightbox is ported. The original combined lightbox/bulk scenario remains.
The short-gallery selection test starts its drag inside the measured gallery section; its
old viewport-based point landed on the footer in both renderers. Selection assertions remain
unchanged. Stage-6 lightbox assertions remain for that stage.

Stage 6 adds Solid selection/mutation, image controls, modal, group-order input and presentation
suites. They cover canonical authority, coalescing, contention/retry, stale reads, query reorder,
page boundaries, identity reuse, dirty drafts, deletion, image/GIF identity, fit/zoom/pan,
clipboard, keyboard priority, sidebar lifetime and modal close/reopen. React assertions and
shared pure helper suites remain enabled. Native-player behavior remains assigned to stage 7.

The native animation lifecycle now runs as one case per media kind, preserving every prior
assertion so image/GIF checks can run before the Solid video port. Sidebar geometry checks wait
for the actual drawer translate transition to settle before asserting exact button dimensions. The new
image comparison suite captures both themes and languages at three widths and verifies
suggestion dismissal and gallery focus restoration through the real window.

## Stage 7 test ownership

Solid `features/lightbox/video/__tests__/createNativeVideoSession.test.ts` ports the nine
React lifecycle cases and adds returned-session early-event filtering, latest-per-type
buffering, observer/frame disposal and retired label failures. `LightboxVideo.test.tsx`
covers native opening, live translations, explicit snapshot attributes, recoverable errors,
acknowledged panel clearance, fullscreen identity and Escape, video-target keys, logical-close
shutdown, late open cleanup, retry and A/B/A navigation, narrow drawer focus timing, and
fullscreen-video to image navigation.
The pure native bounds tests remain shared and the React suites remain enabled.

`video-speed.e2e.js` now also corrupts only its generated original, observes the failed native
player, restores the original and clicks Retry. Pixel comparison verifies decoded, changing
frames after recovery. A mixed-media case checks windowed GIF geometry after fullscreen
video navigation. The existing native controls, fullscreen and 30-cycle checks retain
their assertions. Desktop commands and results are recorded in PROGRESS.md.


## Stage 8 test ownership

Solid `features/settings/__tests__` ports the action/confirmation, startup scanning,
section/status, About and dialog assertions. The grouped SettingsPanel tests exercise the
production controllers directly, preserving the reference section wiring and navigation
assertions without adding its flat-prop compatibility seam. Appearance/Language remain
covered by the earlier shell tests and the shared desktop language suite. Pure progress,
operation-state and duplicate-validation suites remain shared; React suites remain enabled.

`createDuplicateResolver.test.tsx` replaces React setter-spy assertions with observed drafts,
staged changes and commands, including same-turn updates, UUID extensions and reset on
reopen/rescan. The measured virtual-list regression covers removing all groups while old
virtual indices exist. `confirmationPresence.test.tsx` verifies retained counts and paths
during logical close. `operationSafety.test.tsx` covers immediate exclusion, listener failure
and disposal, progress filtering and translation, separate stop-request ownership, root
mapping, and uncertain CSV/restore/clear failure with the real metadata barrier. The library
mutation suite distinguishes restore vocabulary retention from immediate clear reset.

The opt-in `settings-maintenance.e2e.js` runs actual scan and duplicate commands against
disposable media, verifies the operation survives settings remount, cancels and confirms a
mixed rename/delete batch, and checks files plus persisted library rows after reload. Its
scan-lifetime case delays delivery of the real IPC result without replacing the scan or
progress events. Existing startup, settings-language, About and import/export workflow
scenarios run with both renderers. Native file chooser interaction is covered at the mocked
platform boundary; desktop CSV/bundle roundtrips call real commands directly.
