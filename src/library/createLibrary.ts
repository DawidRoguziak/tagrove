import { createEffect, createSignal, onCleanup } from "solid-js";
import { createAssetQuery, type LibraryFilters } from "./query/createAssetQuery";
import { createAssetTagState } from "./metadata/createAssetTagState";
import { createDetailsCache } from "./metadata/createDetailsCache";
import { createMutations } from "./metadata/createMutations";
import { createKnownTags } from "./vocabulary/createKnownTags";

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
  const [range, setRange] = createSignal<LibraryViewport | null>(null);
  const query = createAssetQuery({
    ...options,
    filters: () => filters,
    anchorIndex: () => range()?.visibleStartIndex,
    onQueryChanged: details.clear
  });
  createEffect(
    () => ({ range: range(), epoch: query.queryEpoch }),
    ({ range }) => {
      if (range && !range.paused) query.ensureRange(range.startIndex, range.endIndex);
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
    resetIdentity,
    handleImportDbRestored() {
      resetIdentity();
      query.reset();
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
