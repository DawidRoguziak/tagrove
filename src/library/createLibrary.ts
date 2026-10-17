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
export interface LibraryViewport extends LibraryRange {
  paused: boolean;
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
  const [range, setRange] = createSignal<LibraryViewport | null>(null);
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
      if (range && !range.paused) query.ensureRange(range.startIndex, range.endIndex);
    }
  );
  createEffect(
    () => {
      const viewport = range();
      query.assets;
      const visible: number[] = [];
      const prefetch: number[] = [];
      if (!viewport || viewport.paused) return { visible, prefetch };
      for (
        let index = Math.max(0, viewport.startIndex);
        index <= Math.min(query.total - 1, viewport.endIndex);
        index++
      ) {
        const asset = query.getAssetAt(index);
        if (asset)
          (index >= viewport.visibleStartIndex && index <= viewport.visibleEndIndex
            ? visible
            : prefetch
          ).push(asset.id);
      }
      return { visible, prefetch };
    },
    (demand) => thumbnails.setGalleryThumbnailDemand(demand.visible, demand.prefetch)
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
    handleVirtualRangeChange: (next: LibraryViewport | null) => {
      setRange(next);
    },
    handleGalleryReachEnd() {
      const viewport = range();
      if (viewport && !viewport.paused) query.ensureRange(viewport.startIndex, viewport.endIndex);
    },
    handleRootRemoved: thumbnails.resetThumbnailQueue,
    resetIdentity,
    handleImportDbRestored() {
      resetIdentity();
      query.reset();
      thumbnails.resetThumbnailQueue();
    },
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
