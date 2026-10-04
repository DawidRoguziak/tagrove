import { createEffect, createSignal, onCleanup } from "solid-js";
import { createAssetQuery, type LibraryFilters } from "./query/createAssetQuery";
import { createAssetTagState } from "./metadata/createAssetTagState";
import { createDetailsCache } from "./metadata/createDetailsCache";
import { createMutations } from "./metadata/createMutations";
import { createKnownTags } from "./vocabulary/createKnownTags";
import { createThumbnailQueue } from "./thumbnails/createThumbnailQueue";

export interface LibraryRange {
  startIndex: number;
  endIndex: number;
  visibleStartIndex: number;
  visibleEndIndex: number;
}
const DEFAULT_FILTERS: LibraryFilters = {
  filterInclude: [],
  filterExclude: [],
  appliedMediaKind: "all",
  appliedFavoritesOnly: false
};

export function createLibrary(options: { pageSize?: number; filters?: LibraryFilters } = {}) {
  let filters: LibraryFilters = options.filters ?? DEFAULT_FILTERS;
  const tags = createAssetTagState();
  const details = createDetailsCache(tags);
  const vocabulary = createKnownTags();
  const thumbnails = createThumbnailQueue();
  const [range, setRange] = createSignal<LibraryRange | null>(null);
  const query = createAssetQuery({
    ...options,
    filters: () => filters,
    resetThumbnailQueue: thumbnails.resetThumbnailQueue,
    onQueryChanged: details.clear,
    onAssetsChanged(assets) {
      thumbnails.thumbnailStore.retain(assets.map((asset) => asset.id));
      const paths: Record<number, string> = {};
      for (const asset of assets) if (asset.thumb_path) paths[asset.id] = asset.thumb_path;
      thumbnails.thumbnailStore.complete(paths, []);
    }
  });
  createEffect(
    () => ({ range: range(), epoch: query.queryEpoch }),
    ({ range }) => {
      if (range) query.ensureRange(range.startIndex, range.endIndex);
    }
  );
  createEffect(
    () => ({ range: range(), assets: query.assets }),
    ({ range }) => {
      if (!range) return;
      const visible: number[] = [];
      const prefetch: number[] = [];
      for (
        let index = Math.max(0, range.startIndex);
        index <= Math.min(query.total - 1, range.endIndex);
        index++
      ) {
        const asset = query.getAssetAt(index);
        if (asset)
          (index >= range.visibleStartIndex && index <= range.visibleEndIndex
            ? visible
            : prefetch
          ).push(asset.id);
      }
      thumbnails.setGalleryThumbnailDemand(visible, prefetch);
    }
  );
  const mutations = createMutations({ tags, details, query, vocabulary, filters: () => filters });
  const refreshLibrary = () =>
    Promise.all([query.refresh(), vocabulary.refreshKnownTags()]).then(() => {});
  function resetIdentity() {
    tags.reset();
    details.clear();
  }
  function resetLibrary() {
    resetIdentity();
    query.reset();
    thumbnails.resetThumbnailQueue();
    vocabulary.reset();
    setRange(null);
  }
  let disposed = false;
  onCleanup(() => {
    disposed = true;
  });
  return {
    query,
    tags,
    details,
    vocabulary,
    thumbnails,
    mutations,
    refreshLibrary,
    applyFilters(next: LibraryFilters) {
      filters = {
        ...next,
        filterInclude: [...next.filterInclude],
        filterExclude: [...next.filterExclude],
        metaFilter: next.metaFilter ? { ...next.metaFilter } : null
      };
      return query.refresh();
    },
    handleVirtualRangeChange: (next: LibraryRange) => {
      setRange(next);
    },
    handleRootRemoved: thumbnails.resetThumbnailQueue,
    resetIdentity,
    handleImportDbRestored: resetLibrary,
    handleLibraryCleared: resetLibrary,
    async runMaintenance<T>(kind: "metadata" | "identity", operation: () => Promise<T>) {
      return tags.runWithMutationBarrier(async () => {
        try {
          return await operation();
        } finally {
          if (!disposed) {
            if (kind === "identity") resetLibrary();
            else resetIdentity();
          }
        }
      });
    }
  };
}
