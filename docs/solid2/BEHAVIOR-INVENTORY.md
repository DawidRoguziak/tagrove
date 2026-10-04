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
| `src/components/UI/__tests__/TagEditor.test.tsx` | react | 5, `features/tagging` | Retained, all assertions |
| `src/components/UI/__tests__/ThumbnailImage.test.tsx` | react | 4, `features/gallery` | Retained, all assertions |
| `src/components/UI/__tests__/UiAlert.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/controls/__tests__/controls.test.tsx` |
| `src/components/UI/__tests__/UiModal.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/overlays/__tests__/UiModal.test.tsx` |
| `src/components/UI/__tests__/UiSegmentedControl.test.tsx` | react | 2, `ui/controls` or `ui/overlays` | React assertions retained; Solid counterpart in `src/ui/controls/__tests__/controls.test.tsx` |
| `src/components/app/__tests__/WindowControls.test.tsx` | react | 2, `app/WindowControls` | React assertions retained; Solid counterpart in `src/app/__tests__/WindowControls.test.tsx` |
| `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts` | react | 4, `features/search` | Retained, all assertions |
| `src/components/app/hooks/__tests__/useAssetTagState.test.ts` | react | 3, `library/metadata` | Retained, all assertions |
| `src/components/app/hooks/__tests__/useBulkFavorites.test.ts` | react | 5, `features/bulk` selection and metadata | Retained, all assertions |
| `src/components/app/hooks/__tests__/useBulkSelectionController.test.ts` | react | 5, `features/bulk` selection and metadata | Retained, all assertions |
| `src/components/app/hooks/__tests__/useSelectedSummaries.test.ts` | react | 5, `features/bulk` selection and metadata | Retained, all assertions |
| `src/components/app/services/__tests__/assetMutationService.test.ts` | shared | 3, `library/metadata` | Retained, all assertions |
| `src/components/app/services/__tests__/filterService.test.ts` | shared | 4, `features/search` | Retained, all assertions |
| `src/components/app/services/__tests__/libraryInvalidationService.test.ts` | shared | 3, `library/metadata` | Retained, all assertions |
| `src/ui/dom/__tests__/theme.test.ts` | shared | 2, shared theme persistence | Moved, all assertions retained |
| `src/components/app/services/__tests__/tileSizeService.test.ts` | shared | 4, gallery sizing | Retained, all assertions |
| `src/components/bulk/__tests__/BulkActionsSidebar.test.tsx` | react | 5, `features/bulk` | Retained, all assertions |
| `src/components/bulk/grouping/services/__tests__/applyBulkMediaGroupAction.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/bulk/grouping/services/__tests__/bulkGroupOrderService.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/bulk/tagging/services/__tests__/applyBulkTagsAction.test.ts` | shared | 5, `features/bulk` | Retained, all assertions |
| `src/components/gallery/__tests__/GalleryGrid.test.tsx` | react | 4, `features/gallery` | Retained, all assertions |
| `src/components/gallery/__tests__/GalleryStatusFooter.test.tsx` | react | 4, `features/gallery` | Retained, all assertions |
| `src/components/gallery/hooks/__tests__/useGalleryGridHandlers.test.tsx` | react | 4, `features/gallery` | Retained, all assertions |
| `src/components/gallery/hooks/__tests__/useGalleryVirtualGrid.test.tsx` | react | 4, `features/gallery` | Retained, all assertions |
| `src/components/gallery/services/__tests__/selectionGeometry.test.ts` | shared | 4, `features/gallery` | Retained, all assertions |
| `src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/LightboxMediaStage.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/LightboxModal.copy.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/LightboxModal.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/MediaGroupSetter.test.tsx` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/nativeVideoLayout.test.ts` | shared | 7, `features/lightbox/video` | Retained, all assertions |
| `src/components/lightbox/__tests__/useLightboxImageControls.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/useLightboxTagging.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/__tests__/useNativeVideoSession.test.tsx` | react | 7, `features/lightbox/video` | Retained, all assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxActivity.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxKeyboardShortcuts.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxModalHandlers.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/hooks/__tests__/useLightboxSidebar.test.ts` | react | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/deleteLightboxAssetAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/saveLightboxMediaGroupAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/saveLightboxTagsAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/lightbox/services/__tests__/toggleLightboxFavoriteAction.test.ts` | shared | 5–7, `features/lightbox` and shared tagging | Retained, all assertions |
| `src/components/search/__tests__/SearchTagator.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/__tests__/SearchTagatorWrapper.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/hooks/__tests__/useSearchTagatorHandlers.test.ts` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/services/__tests__/inputService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/services/__tests__/suggestionService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/services/__tests__/tokenService.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/worker/__tests__/SuggestionClient.test.ts` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/search/worker/__tests__/useSuggestions.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
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
| `src/components/tag-list/__tests__/TagListModal.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/components/tag-list/__tests__/TagListSearchLauncher.test.tsx` | react | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/hooks/__tests__/queryRefreshNavigation.test.tsx` | react | 6, `features/lightbox` selection and navigation | Retained, all assertions |
| `src/hooks/__tests__/useLibraryAssets.test.tsx` | react | 3, `library/query` | Retained, all assertions |
| `src/hooks/__tests__/useLibraryBrowser.test.ts` | react | 3, `library/query` | Retained, all assertions |
| `src/hooks/__tests__/useLibraryKnownTags.test.ts` | react | 3, `library/vocabulary` | Retained, all assertions |
| `src/hooks/__tests__/useSelectionState.test.ts` | react | 6, `features/lightbox` selection and navigation | Retained, all assertions |
| `src/hooks/__tests__/useSettingsActions.test.ts` | react | 8, `features/settings` | Retained, all assertions |
| `src/hooks/__tests__/useSettingsActionsConfirmFlow.test.ts` | react | 8, `features/settings` | Retained, all assertions |
| `src/hooks/__tests__/useThumbnailQueue.test.ts` | react | 3, `library/thumbnails` | Retained, all assertions |
| `src/hooks/services/__tests__/thumbnailStore.test.ts` | shared | 3, `library/thumbnails` | Retained, all assertions |
| `src/utils/__tests__/literalTagSearch.test.ts` | shared | 4–5, `features/search` and `features/tagging/suggestions` | Retained, all assertions |
| `src/utils/__tests__/media.test.ts` | shared | 6, framework-independent media helpers | Retained, all assertions |

Stage 2 adds shell mount/disposal, all 11 live translations, theme persistence/native sync,
settings remounts, lazy loading/failure, optional performance cleanup, stale native-window
reads and controlled-input tests. React StrictMode presence coverage remains in the reference;
Solid tests prove cancellation, rapid reopen, disposal and subscription cleanup instead.
