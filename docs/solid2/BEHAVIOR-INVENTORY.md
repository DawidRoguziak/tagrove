# Migration behavior inventory

Reference source commit: `f11126dfafd40f877ecf5e496abc37d77a435e78`.
The original 84 suites and 699 tests are mapped below to their final owners.
The old paths identify reference suites in Git history, not files in the final checkout.
React lifecycle and setter-spy assertions are replaced by Solid ownership, disposal,
canonical publication and user-visible behavior checks. Removed action wrappers are
covered through the actual library mutations and feature models that replaced them.

| Reference suite | Final test owner | Resolution |
| --- | --- | --- |
| `src/__tests__/App.test.tsx` | `src/app/__tests__/runtimeIntegration.test.ts`; `src/features/settings/__tests__/createSettingsActions.test.tsx`; `src/app/__tests__/gallery.test.tsx`; `src/app/__tests__/runtime.test.ts`; `src/app/__tests__/shell.test.tsx`; `src/library/metadata/__tests__/mutations.test.ts`; `src/features/settings/__tests__/operationSafety.test.tsx` | Replaced by model/component behavior assertions |
| `src/platform/tauri/__tests__/api.test.ts` | `src/platform/tauri/__tests__/api.test.ts` | Retained |
| `src/components/UI/__tests__/LazyErrorBoundary.test.tsx` | `src/app/__tests__/shell.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/UI/__tests__/ModalPresence.test.tsx` | `src/ui/overlays/__tests__/UiModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/UI/__tests__/TagEditor.test.tsx` | `src/features/tagging/__tests__/TagEditor.test.tsx` | Ported behavior assertions |
| `src/components/UI/__tests__/ThumbnailImage.test.tsx` | `src/ui/controls/__tests__/ThumbnailImage.test.tsx` | Ported behavior assertions |
| `src/components/UI/__tests__/UiAlert.test.tsx` | `src/ui/controls/__tests__/controls.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/UI/__tests__/UiModal.test.tsx` | `src/ui/overlays/__tests__/UiModal.test.tsx` | Ported behavior assertions |
| `src/components/UI/__tests__/UiSegmentedControl.test.tsx` | `src/ui/controls/__tests__/controls.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/app/__tests__/WindowControls.test.tsx` | `src/app/__tests__/WindowControls.test.tsx` | Ported behavior assertions |
| `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts` | `src/features/search/__tests__/createSearch.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/hooks/__tests__/useAssetTagState.test.ts` | `src/library/metadata/__tests__/createAssetTagState.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/hooks/__tests__/useBulkFavorites.test.ts` | `src/features/bulk/__tests__/createBulk.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/hooks/__tests__/useBulkSelectionController.test.ts` | `src/features/bulk/__tests__/createBulk.test.ts`; `src/features/gallery/__tests__/createSelection.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/hooks/__tests__/useSelectedSummaries.test.ts` | `src/features/bulk/__tests__/createBulk.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/services/__tests__/assetMutationService.test.ts` | `src/library/metadata/__tests__/mutations.test.ts` | Replaced by model/component behavior assertions |
| `src/components/app/services/__tests__/filterService.test.ts` | `src/features/search/__tests__/filterService.test.ts` | Moved with its assertions |
| `src/components/app/services/__tests__/libraryInvalidationService.test.ts` | `src/library/metadata/__tests__/queryImpact.test.ts` | Moved with its assertions |
| `src/ui/dom/__tests__/theme.test.ts` | `src/ui/dom/__tests__/theme.test.ts` | Retained |
| `src/components/app/services/__tests__/tileSizeService.test.ts` | `src/features/gallery/__tests__/tileSizeService.test.ts` | Moved with its assertions |
| `src/components/bulk/__tests__/BulkActionsSidebar.test.tsx` | `src/features/bulk/__tests__/BulkActionsSidebar.test.tsx` | Ported behavior assertions |
| `src/components/bulk/grouping/services/__tests__/applyBulkMediaGroupAction.test.ts` | `src/features/bulk/__tests__/createBulk.test.ts` | Replaced by model/component behavior assertions |
| `src/components/bulk/grouping/services/__tests__/bulkGroupOrderService.test.ts` | `src/features/bulk/__tests__/bulkGroupOrderService.test.ts` | Moved with its assertions |
| `src/components/bulk/tagging/services/__tests__/applyBulkTagsAction.test.ts` | `src/features/bulk/__tests__/createBulk.test.ts` | Replaced by model/component behavior assertions |
| `src/components/gallery/__tests__/GalleryGrid.test.tsx` | `src/features/gallery/__tests__/GalleryGrid.test.tsx` | Ported behavior assertions |
| `src/components/gallery/__tests__/GalleryStatusFooter.test.tsx` | `src/features/gallery/__tests__/GalleryStatusFooter.test.tsx` | Ported behavior assertions |
| `src/components/gallery/hooks/__tests__/useGalleryGridHandlers.test.tsx` | `src/features/gallery/__tests__/createGridHandlers.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/gallery/hooks/__tests__/useGalleryVirtualGrid.test.tsx` | `src/features/gallery/__tests__/createVirtualGrid.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/gallery/services/__tests__/selectionGeometry.test.ts` | `src/features/gallery/__tests__/selectionGeometry.test.ts` | Moved with its assertions |
| `src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx` | `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/__tests__/LightboxMediaStage.test.tsx` | `src/features/lightbox/__tests__/imageControls.test.tsx`; `src/features/lightbox/video/__tests__/LightboxVideo.test.tsx`; `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/__tests__/LightboxModal.copy.test.tsx` | `src/features/lightbox/__tests__/presentation.test.ts`; `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/__tests__/LightboxModal.test.tsx` | `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Ported behavior assertions |
| `src/components/lightbox/__tests__/MediaGroupSetter.test.tsx` | `src/features/lightbox/__tests__/MediaGroupSetter.test.tsx` | Ported behavior assertions |
| `src/components/lightbox/__tests__/nativeVideoLayout.test.ts` | `src/features/lightbox/video/__tests__/nativeVideoLayout.test.ts` | Moved with its assertions |
| `src/components/lightbox/__tests__/useLightboxImageControls.test.ts` | `src/features/lightbox/__tests__/imageControls.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/__tests__/useLightboxTagging.test.ts` | `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/__tests__/useNativeVideoSession.test.tsx` | `src/features/lightbox/video/__tests__/createNativeVideoSession.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxActivity.test.ts` | `src/features/lightbox/__tests__/presentation.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxKeyboardShortcuts.test.ts` | `src/features/lightbox/__tests__/keyboard.test.tsx`; `src/features/lightbox/__tests__/imageControls.test.tsx`; `src/features/lightbox/video/__tests__/LightboxVideo.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxModalHandlers.test.ts` | `src/features/lightbox/__tests__/createLightbox.test.ts`; `src/features/lightbox/__tests__/LightboxModal.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxSidebar.test.ts` | `src/features/lightbox/__tests__/presentation.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/services/__tests__/deleteLightboxAssetAction.test.ts` | `src/features/lightbox/__tests__/createLightbox.test.ts`; `src/library/metadata/__tests__/mutations.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/services/__tests__/saveLightboxMediaGroupAction.test.ts` | `src/features/lightbox/__tests__/createLightbox.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/services/__tests__/saveLightboxTagsAction.test.ts` | `src/features/lightbox/__tests__/createLightbox.test.ts`; `src/library/metadata/__tests__/mutations.test.ts` | Replaced by model/component behavior assertions |
| `src/components/lightbox/services/__tests__/toggleLightboxFavoriteAction.test.ts` | `src/library/metadata/__tests__/mutations.test.ts`; `src/features/lightbox/__tests__/createLightbox.test.ts` | Replaced by model/component behavior assertions |
| `src/components/search/__tests__/SearchTagator.test.tsx` | `src/features/search/__tests__/SearchTagator.test.tsx` | Ported behavior assertions |
| `src/components/search/__tests__/SearchTagatorWrapper.test.tsx` | `src/features/search/__tests__/SearchTagatorWrapper.test.tsx` | Ported behavior assertions |
| `src/components/search/hooks/__tests__/useSearchTagatorHandlers.test.ts` | `src/features/search/__tests__/SearchTagator.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/search/services/__tests__/inputService.test.ts` | `src/features/search/suggestions/__tests__/inputService.test.ts` | Moved with its assertions |
| `src/components/search/services/__tests__/suggestionService.test.ts` | `src/features/search/suggestions/__tests__/suggestionService.test.ts` | Moved with its assertions |
| `src/components/search/services/__tests__/tokenService.test.ts` | `src/features/search/suggestions/__tests__/tokenService.test.ts` | Moved with its assertions |
| `src/components/search/worker/__tests__/SuggestionClient.test.ts` | `src/features/search/suggestions/__tests__/SuggestionClient.test.ts` | Ported behavior assertions |
| `src/components/search/worker/__tests__/useSuggestions.test.tsx` | `src/features/search/suggestions/__tests__/createSuggestions.test.ts` | Replaced by model/component behavior assertions |
| `src/components/settings/__tests__/ClearLibraryConfirmDialog.test.tsx` | `src/features/settings/__tests__/ClearLibraryConfirmDialog.test.tsx` | Ported behavior assertions |
| `src/components/settings/__tests__/DuplicateResolverDialog.test.tsx` | `src/features/settings/__tests__/DuplicateResolverDialog.test.tsx` | Ported behavior assertions |
| `src/components/settings/__tests__/RemoveScanRootConfirmDialog.test.tsx` | `src/features/settings/__tests__/RemoveScanRootConfirmDialog.test.tsx` | Ported behavior assertions |
| `src/components/settings/__tests__/SectionOperationStatus.test.tsx` | `src/features/settings/__tests__/SectionOperationStatus.test.tsx` | Ported behavior assertions |
| `src/components/settings/__tests__/SettingsPanel.test.tsx` | `src/features/settings/__tests__/SettingsPanel.test.tsx` | Ported behavior assertions |
| `src/components/settings/hooks/__tests__/useDuplicateResolverHandlers.test.ts` | `src/features/settings/__tests__/createDuplicateResolver.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/settings/hooks/__tests__/useStartupScan.test.tsx` | `src/features/settings/__tests__/startupScan.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/settings/sections/__tests__/AboutSection.test.tsx` | `src/features/settings/__tests__/AboutSection.test.tsx` | Ported behavior assertions |
| `src/components/settings/sections/__tests__/AppearanceSection.test.tsx` | `src/app/__tests__/shell.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/settings/sections/__tests__/DangerZoneSection.test.tsx` | `src/features/settings/__tests__/DangerZoneSection.test.tsx` | Ported behavior assertions |
| `src/components/settings/sections/__tests__/DuplicatesSection.test.tsx` | `src/features/settings/__tests__/DuplicatesSection.test.tsx` | Ported behavior assertions |
| `src/components/settings/sections/__tests__/ImportExportSection.test.tsx` | `src/features/settings/__tests__/ImportExportSection.test.tsx` | Ported behavior assertions |
| `src/components/settings/sections/__tests__/LanguageSection.test.tsx` | `src/app/__tests__/shell.test.tsx` | Replaced by model/component behavior assertions |
| `src/components/settings/sections/__tests__/ScanSettingsSection.test.tsx` | `src/features/settings/__tests__/ScanSettingsSection.test.tsx` | Ported behavior assertions |
| `src/components/settings/services/__tests__/duplicateValidationService.test.ts` | `src/features/settings/__tests__/duplicateValidationService.test.ts` | Moved with its assertions |
| `src/components/settings/services/__tests__/operationStateService.test.ts` | `src/features/settings/__tests__/operationStateService.test.ts` | Moved with its assertions |
| `src/components/settings/services/__tests__/progressService.test.ts` | `src/features/settings/__tests__/progressService.test.ts` | Moved with its assertions |
| `src/components/tag-list/__tests__/TagListModal.test.tsx` | `src/features/search/tag-list/__tests__/TagListModal.test.tsx` | Ported behavior assertions |
| `src/components/tag-list/__tests__/TagListSearchLauncher.test.tsx` | `src/features/search/tag-list/__tests__/TagListModal.test.tsx`; `src/app/__tests__/gallery.test.tsx` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/queryRefreshNavigation.test.tsx` | `src/features/lightbox/__tests__/createLightbox.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useLibraryAssets.test.tsx` | `src/library/query/__tests__/createAssetQuery.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useLibraryBrowser.test.ts` | `src/library/__tests__/createLibrary.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useLibraryKnownTags.test.ts` | `src/library/vocabulary/__tests__/createKnownTags.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useSelectionState.test.ts` | `src/features/lightbox/__tests__/createLightbox.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useSettingsActions.test.ts` | `src/features/settings/__tests__/createSettingsActions.test.tsx` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useSettingsActionsConfirmFlow.test.ts` | `src/features/settings/__tests__/createSettingsActionsConfirmFlow.test.tsx` | Replaced by model/component behavior assertions |
| `src/hooks/__tests__/useThumbnailQueue.test.ts` | `src/library/thumbnails/__tests__/createThumbnailQueue.test.ts` | Replaced by model/component behavior assertions |
| `src/hooks/services/__tests__/thumbnailStore.test.ts` | `src/library/thumbnails/__tests__/thumbnailStore.test.ts` | Moved with its assertions |
| `src/utils/__tests__/literalTagSearch.test.ts` | `src/utils/__tests__/literalTagSearch.test.ts` | Retained |
| `src/utils/__tests__/media.test.ts` | `src/utils/__tests__/media.test.ts` | Retained |

## Historical stage notes

These notes describe intermediate commits. The final ownership table above supersedes
references below to retained React suites or temporary Vitest projects.

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

## Stage 9 completion

The final suite runs only Solid and pure tests. Startup vocabulary loading and its failure
logging are covered in `src/app/__tests__/runtime.test.ts`. The real application regression
in `src/app/__tests__/lightboxRecovery.test.tsx` proves failed-view reopen recovery and
healthy media ownership across navigation. Lightbox model tests retain all no-selection
mutation guards. The 84 reference suites above have final owners; no behavior gap remains
open in this inventory.
