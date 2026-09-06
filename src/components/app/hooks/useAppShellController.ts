import { useCallback, useEffect, useRef, useState } from "react";
import {
  DEFAULT_TILE_SIZE,
  TILE_SIZE_MAX,
  TILE_SIZE_MIN,
  TILE_SIZE_STEP,
  clampTileSize,
  getNextTileSizeByWheel
} from "../services/tileSizeService";
import { useAppSearchFilters } from "./useAppSearchFilters";
import { useBulkSelectionController } from "./useBulkSelectionController";
import { useAssetTagState } from "./useAssetTagState";
import { useAppLanguage } from "../../../hooks/useAppLanguage";
import { useAppTheme } from "../../../hooks/useAppTheme";
import { useLibraryBrowser } from "../../../hooks/useLibraryBrowser";
import { useSelectionState } from "../../../hooks/useSelectionState";
import { useSettingsActions } from "../../../hooks/useSettingsActions";
import { useSettingsView } from "../../../hooks/useSettingsView";

const PAGE_SIZE = 128;

export function useAppShellController() {
  const searchFilters = useAppSearchFilters();
  const [tileSize, setTileSize] = useState(DEFAULT_TILE_SIZE);
  const [highlightScanSection, setHighlightScanSection] = useState(false);
  const { theme, setTheme } = useAppTheme();
  const { language, setLanguage } = useAppLanguage();
  const { settingsViewOpen, openSettingsView, closeSettingsView } = useSettingsView();
  const assetTagState = useAssetTagState();
  const appScrollRef = useRef<HTMLElement | null>(null);
  const tileSizeFrameRef = useRef<number | null>(null);
  const tileWheelFrameRef = useRef<number | null>(null);
  const pendingTileSizeRef = useRef<number | null>(null);
  const pendingWheelDeltaRef = useRef(0);

  const library = useLibraryBrowser({
    pageSize: PAGE_SIZE,
    filterInclude: searchFilters.appliedParsedFilter.include,
    filterExclude: searchFilters.appliedParsedFilter.exclude,
    metaFilter: searchFilters.appliedParsedFilter.metaFilter,
    appliedMediaKind: searchFilters.appliedMediaKind,
    appliedFavoritesOnly: searchFilters.appliedFavoritesOnly
  });

  const handleImportDbRestored = useCallback(() => {
    assetTagState.reset();
    library.handleImportDbRestored();
  }, [assetTagState, library.handleImportDbRestored]);

  const handleLibraryCleared = useCallback(() => {
    assetTagState.reset();
    library.handleLibraryCleared();
  }, [assetTagState, library.handleLibraryCleared]);

  const handleTagCacheInvalidated = useCallback(() => {
    assetTagState.reset();
  }, [assetTagState]);

  const settingsActions = useSettingsActions({
    setLoading: library.setLoading,
    refreshLibrary: library.refreshLibrary,
    onRootRemoved: library.handleRootRemoved,
    onImportDbRestored: handleImportDbRestored,
    onLibraryCleared: handleLibraryCleared,
    onTagCacheInvalidated: handleTagCacheInvalidated,
    runWithTagMutationBarrier: assetTagState.runWithMutationBarrier,
    duplicateThumbs: library.thumbs,
    duplicateRenderingThumbnailIds: library.renderingAssetIds,
    onQueueDuplicateThumbnailsByIds: library.queueThumbnailsByIds
  });

  const selection = useSelectionState({
    assets: library.assets,
    setAssets: library.setAssets,
    appliedFavoritesOnly: searchFilters.appliedFavoritesOnly,
    refresh: library.refresh,
    refreshKnownTags: library.refreshKnownTags,
    assetTagState,
    assetCount: library.assetCount,
    queryEpoch: library.queryEpoch,
    getAssetAtAsync: library.getAssetAtAsync,
    getAssetIndex: library.getAssetIndex,
    appliedFilter: searchFilters.appliedDescriptor
  });

  const bulkSelection = useBulkSelectionController({
    appliedFavoritesOnly: searchFilters.appliedFavoritesOnly,
    onFavoritesChanged: selection.applyFavoriteChanges,
    assets: library.assets,
    queryEpoch: library.queryEpoch,
    getIdsRangeAsync: library.getIdsRangeAsync,
    knownTags: library.knownTags,
    settingsViewOpen,
    queueThumbnailsByIds: library.queueThumbnailsByIds,
    setAssets: library.setAssets,
    refresh: library.refresh,
    refreshKnownTags: library.refreshKnownTags,
    assetTagState,
    appliedFilter: searchFilters.appliedDescriptor
  });

  const onSearchSubmit = useCallback(() => {
    void searchFilters.handleSearchSubmit(library.refresh).catch(() => {});
  }, [library.refresh, searchFilters]);

  const onApplyTagListSearch = useCallback(
    async (nextFilterInput: string) => {
      await searchFilters.applyFilterInputAndSubmit(nextFilterInput, library.refresh);
    },
    [library.refresh, searchFilters]
  );

  const onClearSearch = useCallback(() => {
    void searchFilters.handleClearSearch(library.refresh).catch(() => {});
  }, [library.refresh, searchFilters]);

  const onMediaKindChange = useCallback(
    async (value: typeof searchFilters.mediaKind) => {
      await searchFilters.applyMediaKindAndSubmit(value, library.refresh);
    },
    [library.refresh, searchFilters]
  );

  const onFavoritesOnlyChange = useCallback(
    async (value: boolean) => {
      await searchFilters.applyFavoritesOnlyAndSubmit(value, library.refresh);
    },
    [library.refresh, searchFilters]
  );

  const onTileSizeChange = useCallback((nextSize: number) => {
    pendingTileSizeRef.current = clampTileSize(nextSize);
    if (tileSizeFrameRef.current !== null) return;
    tileSizeFrameRef.current = window.requestAnimationFrame(() => {
      tileSizeFrameRef.current = null;
      const pending = pendingTileSizeRef.current;
      pendingTileSizeRef.current = null;
      if (pending !== null) setTileSize(pending);
    });
  }, []);

  const onTileZoomByWheel = useCallback((deltaY: number) => {
    pendingWheelDeltaRef.current += deltaY;
    if (tileWheelFrameRef.current !== null) return;
    tileWheelFrameRef.current = window.requestAnimationFrame(() => {
      tileWheelFrameRef.current = null;
      const pendingDelta = pendingWheelDeltaRef.current;
      pendingWheelDeltaRef.current = 0;
      setTileSize((prevSize) => getNextTileSizeByWheel(prevSize, pendingDelta));
    });
  }, []);

  useEffect(() => {
    return () => {
      if (tileSizeFrameRef.current !== null) window.cancelAnimationFrame(tileSizeFrameRef.current);
      if (tileWheelFrameRef.current !== null) window.cancelAnimationFrame(tileWheelFrameRef.current);
    };
  }, []);

  const onOpenSettingsView = useCallback(() => {
    setHighlightScanSection(false);
    openSettingsView();
  }, [openSettingsView]);

  const onAddFirstFolder = useCallback(() => {
    setHighlightScanSection(true);
    openSettingsView();
  }, [openSettingsView]);

  const onCloseSettingsView = useCallback(() => {
    setHighlightScanSection(false);
    closeSettingsView();
  }, [closeSettingsView]);

  useEffect(() => {
    if (!settingsViewOpen && highlightScanSection) {
      setHighlightScanSection(false);
    }
  }, [highlightScanSection, settingsViewOpen]);

  // Startup runs once per shell lifetime; callbacks are captured for that lifecycle.
  const startup = useRef({ hydrate: library.hydrateKnownTags, scan: settingsActions.scan.scanOnStartup, roots: settingsActions.scan.refreshScanRoots });
  useEffect(() => {
    void startup.current.scan();
    void Promise.all([
      startup.current.hydrate(),
      startup.current.roots().catch(() => [])
    ]).catch(() => {});
  }, []);

  const refreshRef = useRef(library.refresh);
  refreshRef.current = library.refresh;
  // biome-ignore lint/correctness/useExhaustiveDependencies: normalized query identity triggers refresh using the current callback; replay must restart cancelled reads.
  useEffect(() => {
    void refreshRef.current().catch(() => {});
  }, [
    searchFilters.appliedQueryKey
  ]);

  const onLoadRetry = useCallback(() => {
    void library.retryLoad().catch(() => {});
  }, [library.retryLoad]);

  return {
    settingsViewOpen,
    galleryView: {
      search: {
        filterInput: searchFilters.filterInput,
        onFilterChange: searchFilters.handleFilterChange,
        filterValidationError: searchFilters.filterValidationError,
        knownTags: library.knownTags,
        mediaKind: searchFilters.mediaKind,
        onMediaKindChange,
        onSearchSubmit,
        onApplyTagListSearch,
        onClearSearch,
        favoritesOnly: searchFilters.favoritesOnly,
        onFavoritesOnlyChange
      },
      media: {
        assets: library.assets,
        assetCount: library.assetCount,
        getAssetAt: library.getAssetAt,
        selectedId: selection.selected?.id ?? null,
        thumbs: library.thumbs,
        tileSize,
        tileSizeMin: TILE_SIZE_MIN,
        tileSizeMax: TILE_SIZE_MAX,
        tileSizeStep: TILE_SIZE_STEP,
        hasMore: false,
        isLoading: library.loading,
        isGeneratingThumbnails: library.isGeneratingPage,
        pendingThumbnailCount: library.pendingPageSize,
        renderingThumbnailIds: library.renderingAssetIds,
        thumbnailStore: library.thumbnailStore,
        scrollContainerRef: appScrollRef,
        onReachEnd: library.handleReachEnd,
        onVirtualRangeChange: library.handleVirtualRangeChange,
        onCtrlWheelZoom: onTileZoomByWheel,
        onTileSizeChange,
        onSelect: selection.setSelected,
        hasScanRoots: settingsActions.scan.scanRoots.length > 0,
        onAddFirstFolder,
        loadError: library.loadError,
        onLoadRetry,
        pageFailureEpoch: library.pageFailureEpoch
      },
      bulkSelection,
      onOpenSettingsView
    },
    settingsView: {
      onBack: onCloseSettingsView,
      highlightScanSection,
      appearance: {
        theme,
        onThemeChange: setTheme,
        language,
        onLanguageChange: setLanguage
      },
      scan: settingsActions.scan,
      importExport: settingsActions.importExport,
      dangerZone: settingsActions.dangerZone,
      duplicates: settingsActions.duplicates
    },
    lightbox: {
      selected: selection.selected,
      tagEditor: selection.tagEditor,
      onTagEditorChange: selection.setTagEditor,
      onSaveTags: selection.saveTags,
      onRetryTags: selection.retryTags,
      tagSaving: selection.tagSaving,
      tagFailed: selection.tagFailed,
      tagDetailsLoading: selection.tagDetailsLoading,
      tagDetailsFailed: selection.tagDetailsFailed,
      assetDetailsFailed: selection.assetDetailsFailed,
      onRetryTagDetails: selection.retryTagDetails,
      mediaGroupKeyEditor: selection.mediaGroupKeyEditor,
      mediaGroupOrderEditor: selection.mediaGroupOrderEditor,
      onMediaGroupKeyEditorChange: selection.setMediaGroupKeyEditor,
      onMediaGroupOrderEditorChange: selection.setMediaGroupOrderEditor,
      onSaveMediaGroup: selection.saveMediaGroup,
      knownTags: library.knownTags,
      onNavigatePrevious: selection.handleSelectPrevious,
      onNavigateNext: selection.handleSelectNext,
      onToggleFavorite: selection.toggleSelectedFavorite,
      favoritePending: selection.favoritePending,
      groupPending: selection.groupPending,
      favoriteFailed: selection.favoriteFailed,
      groupFailed: selection.groupFailed,
      onDeleteMedia: selection.deleteSelectedAsset,
      onClose: () => selection.setSelected(null)
    }
  };
}
