import { useCallback, useMemo, useState } from "react";
import type { SearchMediaKind } from "../components/app/types";
import type { SearchMetaFilter } from "../types";
import { useLibraryAssets } from "./useLibraryAssets";
import { useLibraryKnownTags } from "./useLibraryKnownTags";
import { useLibraryLifecycle } from "./useLibraryLifecycle";
import { useThumbnailQueue } from "./useThumbnailQueue";

interface UseLibraryBrowserArgs {
  pageSize: number;
  filterInclude: string[];
  filterExclude: string[];
  metaFilter?: SearchMetaFilter | null;
  appliedMediaKind: SearchMediaKind;
  appliedFavoritesOnly: boolean;
}

export function useLibraryBrowser({
  pageSize,
  filterInclude,
  filterExclude,
  metaFilter = null,
  appliedMediaKind,
  appliedFavoritesOnly
}: UseLibraryBrowserArgs) {
  const [thumbs, setThumbs] = useState<Record<number, string>>({});
  const {
    queueThumbnailsByIds,
    resetThumbnailQueue,
    isGeneratingPage,
    pendingPageSize,
    renderingAssetIds,
    thumbnailStore
  } = useThumbnailQueue({
    thumbs,
    setThumbs
  });

  const assetsState = useLibraryAssets({
    pageSize,
    filterInclude,
    filterExclude,
    metaFilter,
    appliedMediaKind,
    appliedFavoritesOnly,
    setThumbs,
    resetThumbnailQueue
  });

  const knownTagsState = useLibraryKnownTags();
  const lifecycle = useLibraryLifecycle({
    resetThumbnailQueue,
    setThumbs,
    setAssets: assetsState.setAssets,
    setTotal: assetsState.setTotal,
    setOffset: assetsState.setOffset,
    setKnownTags: knownTagsState.setKnownTags
  });

  const refreshLibrary = useCallback(async () => {
    await Promise.all([assetsState.refresh(), knownTagsState.refreshKnownTags()]);
  }, [assetsState.refresh, knownTagsState.refreshKnownTags]);

  const handleVirtualRangeChange = useCallback(
    (startIndex: number, endIndex: number) => {
      assetsState.ensureRange(startIndex, endIndex);
      const from = Math.max(0, startIndex);
      const to = Math.min(assetsState.total - 1, endIndex);
      if (from > to) {
        return;
      }

      const ids: number[] = [];
      for (let index = from; index <= to; index += 1) {
        const asset = assetsState.getAssetAt(index);
        if (asset) ids.push(asset.id);
      }
      queueThumbnailsByIds(ids);
    },
    [assetsState.ensureRange, assetsState.getAssetAt, assetsState.total, queueThumbnailsByIds]
  );

  return useMemo(
    () => ({
      assets: assetsState.assets,
      assetCount: assetsState.total,
      getAssetAt: assetsState.getAssetAt,
      getAssetAtAsync: assetsState.getAssetAtAsync,
      getAssetIndex: assetsState.getAssetIndex,
      setAssets: assetsState.setAssets,
      setLoading: assetsState.setLoading,
      thumbs,
      total: assetsState.total,
      loading: assetsState.loading,
      knownTags: knownTagsState.knownTags,
      refresh: assetsState.refresh,
      refreshKnownTags: knownTagsState.refreshKnownTags,
      hydrateKnownTags: knownTagsState.hydrateKnownTags,
      refreshLibrary,
      loadError: assetsState.loadError,
      retryLoad: assetsState.retryLoad,
      pageFailureEpoch: assetsState.pageFailureEpoch,
      queueThumbnailsByIds,
      handleReachEnd: assetsState.handleReachEnd,
      handleVirtualRangeChange,
      handleRootRemoved: lifecycle.handleRootRemoved,
      handleImportDbRestored: lifecycle.handleImportDbRestored,
      handleLibraryCleared: lifecycle.handleLibraryCleared,
      isGeneratingPage,
      pendingPageSize,
      renderingAssetIds,
      thumbnailStore
    }),
    [
      assetsState.assets,
      assetsState.handleReachEnd,
      assetsState.getAssetAt,
      assetsState.getAssetAtAsync,
      assetsState.getAssetIndex,
      assetsState.loading,
      assetsState.refresh,
      assetsState.setAssets,
      assetsState.setLoading,
      assetsState.total,
      handleVirtualRangeChange,
      isGeneratingPage,
      knownTagsState.hydrateKnownTags,
      knownTagsState.knownTags,
      knownTagsState.refreshKnownTags,
      lifecycle.handleImportDbRestored,
      lifecycle.handleLibraryCleared,
      lifecycle.handleRootRemoved,
      assetsState.loadError,
      assetsState.pageFailureEpoch,
      assetsState.retryLoad,
      pendingPageSize,
      queueThumbnailsByIds,
      refreshLibrary,
      renderingAssetIds,
      thumbnailStore,
      thumbs
    ]
  );
}


