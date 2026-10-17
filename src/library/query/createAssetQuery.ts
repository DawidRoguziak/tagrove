import { createMemo, createSignal, onCleanup } from "solid-js";
import {
  getAssetQueryPage,
  getAssetQueryPosition,
  startAssetQuery
} from "../../platform/tauri/assets";
import type {
  AssetQueryPositionResult,
  AssetSummary,
  SearchMetaFilter,
  MediaKind
} from "../../platform/tauri/contracts/assets";

const MAX_CACHED_PAGES = 12;

interface PageCache {
  pages: Map<number, number[]>;
  assetsById: Map<number, AssetSummary>;
  lru: number[];
}

export interface LibraryFilters {
  filterInclude: string[];
  filterExclude: string[];
  metaFilter?: SearchMetaFilter | null;
  appliedMediaKind: "all" | MediaKind;
  appliedFavoritesOnly: boolean;
}
interface Options {
  filters: () => LibraryFilters;
  pageSize?: number;
  resetThumbnailQueue: () => void;
  onQueryChanged?: () => void;
  onAssetsChanged?: (assets: AssetSummary[]) => void;
}

const EMPTY_CACHE: PageCache = {
  pages: new Map(),
  assetsById: new Map(),
  lru: []
};

export function createAssetQuery({
  pageSize = 128,
  filters,
  resetThumbnailQueue,
  onQueryChanged,
  onAssetsChanged
}: Options) {
  const ownerId = crypto.randomUUID();
  let state = {
    cache: EMPTY_CACHE,
    total: 0,
    offset: 0,
    loading: false,
    loadError: null as string | null,
    pageFailureEpoch: 0,
    queryPending: false,
    queryEpoch: 0
  };
  const [snapshot, setSnapshot] = createSignal(state);
  const visibleCache = createMemo(() => snapshot().cache);
  const assets = createMemo(() => Array.from(visibleCache().assetsById.values()));
  const readTotal = createMemo(() => snapshot().total);
  const readOffset = createMemo(() => snapshot().offset);
  const readLoading = createMemo(() => snapshot().loading);
  const readLoadError = createMemo(() => snapshot().loadError);
  const readPageFailureEpoch = createMemo(() => snapshot().pageFailureEpoch);
  const readQueryEpoch = createMemo(() => snapshot().queryEpoch);
  const readQueryPending = createMemo(() => snapshot().queryPending);
  let disposed = false;
  function publish(patch: Partial<typeof state>) {
    state = { ...state, ...patch };
    setSnapshot(state);
    if (patch.cache) onAssetsChanged?.(Array.from(patch.cache.assetsById.values()));
    if (patch.queryEpoch !== undefined) onQueryChanged?.();
  }
  let currentSessionId: number | null = null;
  let currentGeneration = 0;
  let currentPatchCounter = 0;
  const inFlightPages: Map<number, Promise<AssetSummary[] | undefined>> = new Map();
  const failedPages = new Map<number, Error>();
  let inFlightCount = 0;
  let activePages = new Set<number>();
  let accessOrder: number[] = [];
  const touchPage = (pageOffset: number) => {
    accessOrder = [pageOffset, ...accessOrder.filter((offset) => offset !== pageOffset)].slice(
      0,
      MAX_CACHED_PAGES
    );
  };

  const invalidateReads = () => {
    currentGeneration += 1;
    currentSessionId = null;
    inFlightPages.clear();
    failedPages.clear();
    inFlightCount = 0;
  };
  onCleanup(() => {
    disposed = true;
    invalidateReads();
  });
  const reset = () => {
    invalidateReads();
    activePages.clear();
    accessOrder = [];
    publish({
      cache: EMPTY_CACHE,
      total: 0,
      offset: 0,
      loading: false,
      queryPending: false,
      loadError: null,
      queryEpoch: state.queryEpoch + 1
    });
  };

  const beginLoading = () => {
    inFlightCount += 1;
    publish({ loading: true });
  };
  const endLoading = () => {
    inFlightCount = Math.max(0, inFlightCount - 1);
    publish({ loading: inFlightCount > 0 });
  };

  // Summaries are stored verbatim: no path/size/tags fabrication happens here.
  const mergePage = (pageOffset: number, summaries: AssetSummary[], replace: boolean) => {
    touchPage(pageOffset);
    const previous = state.cache;
    const pages = replace ? new Map<number, number[]>() : new Map(previous.pages);
    const assetsById = replace ? new Map<number, AssetSummary>() : new Map(previous.assetsById);
    pages.set(
      pageOffset,
      summaries.map((summary) => summary.id)
    );
    for (const summary of summaries) assetsById.set(summary.id, summary);
    const lru = [...new Set([...accessOrder, pageOffset, ...(replace ? [] : previous.lru)])].filter(
      (offset) => pages.has(offset)
    );
    while (lru.length > MAX_CACHED_PAGES) {
      let candidate = lru.length - 1;
      while (candidate >= 0 && activePages.has(lru[candidate])) candidate--;
      const [evictedOffset] = lru.splice(candidate < 0 ? lru.length - 1 : candidate, 1);
      if (evictedOffset === undefined) break;
      const evictedIds = pages.get(evictedOffset) ?? [];
      pages.delete(evictedOffset);
      for (const id of evictedIds) assetsById.delete(id);
    }
    publish({
      cache: { pages, assetsById, lru },
      offset: Math.max(state.offset, pageOffset + summaries.length)
    });
  };

  const refresh = async () => {
    if (disposed) return;
    const {
      filterInclude,
      filterExclude,
      metaFilter = null,
      appliedMediaKind,
      appliedFavoritesOnly
    } = filters();
    invalidateReads();
    publish({ queryPending: true });
    publish({ queryEpoch: state.queryEpoch + 1 });
    const generation = currentGeneration;
    currentSessionId = null;
    inFlightPages.clear();
    activePages.clear();
    accessOrder = [];
    resetThumbnailQueue();
    beginLoading();
    const perfMark = `asset-query-${generation}`;
    if (import.meta.env.VITE_MEDIATAGGER_PERF === "1") performance.mark(`${perfMark}-start`);
    try {
      let patchCounter = currentPatchCounter;
      let result:
        | Awaited<ReturnType<typeof startAssetQuery>>
        | Awaited<ReturnType<typeof getAssetQueryPage>> = await startAssetQuery({
        tagsAnd: [...filterInclude],
        tagsNot: [...filterExclude],
        mediaKind: appliedMediaKind,
        favoritesOnly: appliedFavoritesOnly,
        metaFilter: metaFilter ? { ...metaFilter } : null,
        ownerId,
        generation,
        pageSize
      });
      while (
        generation === currentGeneration &&
        result.status === "ready" &&
        patchCounter !== currentPatchCounter
      ) {
        patchCounter = currentPatchCounter;
        result = await getAssetQueryPage(result.session_id, result.offset, pageSize);
      }
      if (generation !== currentGeneration) return;
      if (result.status === "superseded") {
        throw new Error("Query was superseded. Retry your search.");
      }
      if (result.status === "stale") {
        await refresh();
        return;
      }
      currentSessionId = result.session_id;
      publish({ queryEpoch: state.queryEpoch + 1 });
      publish({ total: result.total });
      publish({ offset: result.items.length });
      publish({ loadError: null });
      mergePage(0, result.items, true);
    } catch (error) {
      if (generation === currentGeneration) {
        publish({ loadError: error instanceof Error ? error.message : String(error) });
        publish({ pageFailureEpoch: state.pageFailureEpoch + 1 });
      }
      throw error instanceof Error ? error : new Error(String(error));
    } finally {
      if (import.meta.env.VITE_MEDIATAGGER_PERF === "1") {
        performance.mark(`${perfMark}-end`);
        performance.measure("asset-query-refresh", `${perfMark}-start`, `${perfMark}-end`);
      }
      if (generation === currentGeneration) {
        publish({ queryPending: false });
        endLoading();
      }
    }
  };

  const loadPage = async (pageOffset: number) => {
    const sessionId = currentSessionId;
    if (sessionId === null || pageOffset < 0 || pageOffset >= state.total) return undefined;
    touchPage(pageOffset);
    if (state.cache.pages.has(pageOffset)) {
      if (state.cache.pages.get(pageOffset)?.some((id) => !state.cache.assetsById.has(id)))
        return undefined;
      return (state.cache.pages.get(pageOffset) ?? [])
        .map((assetId) => state.cache.assetsById.get(assetId))
        .filter((summary): summary is AssetSummary => Boolean(summary));
    }
    const failure = failedPages.get(pageOffset);
    if (failure) throw failure;
    const inFlight = inFlightPages.get(pageOffset);
    if (inFlight) return inFlight;

    const generation = currentGeneration;
    let request!: Promise<AssetSummary[] | undefined>;
    request = (async () => {
      beginLoading();
      try {
        let patchCounter = currentPatchCounter;
        let result = await getAssetQueryPage(sessionId, pageOffset, pageSize);
        while (
          generation === currentGeneration &&
          result.status === "ready" &&
          patchCounter !== currentPatchCounter
        ) {
          patchCounter = currentPatchCounter;
          result = await getAssetQueryPage(result.session_id, result.offset, pageSize);
        }
        if (generation !== currentGeneration) return undefined;
        if (result.status === "stale") {
          await refresh();
          return undefined;
        }
        if (failedPages.size === 0) publish({ loadError: null });
        mergePage(result.offset, result.items, false);
        return result.items;
      } catch (error) {
        // Demand must not retry this hole until the user explicitly retries.
        if (generation === currentGeneration) {
          failedPages.set(pageOffset, error instanceof Error ? error : new Error(String(error)));
          publish({ loadError: error instanceof Error ? error.message : String(error) });
          publish({ pageFailureEpoch: state.pageFailureEpoch + 1 });
        }
        throw error instanceof Error ? error : new Error(String(error));
      } finally {
        if (inFlightPages.get(pageOffset) === request) {
          inFlightPages.delete(pageOffset);
        }
        if (generation === currentGeneration) endLoading();
      }
    })();
    inFlightPages.set(pageOffset, request);
    return request;
  };

  const getAssetPosition = async (assetId: number): Promise<AssetQueryPositionResult> => {
    const generation = currentGeneration;
    const sessionId = currentSessionId;
    if (sessionId === null) throw new Error("Asset query is unavailable");
    const result = await getAssetQueryPosition(sessionId, assetId);
    if (generation !== currentGeneration) return { status: "stale" };
    if (result.status === "stale") await refresh();
    return result;
  };

  const retryLoad = async () => {
    if (currentSessionId === null) {
      await refresh();
      return;
    }
    const offsets = [...failedPages.keys()].filter((offset) => activePages.has(offset));
    // Explicit Retry also unblocks offscreen pages needed by a selection read.
    // Only viewport demand is fetched here; the selection retries its own ranges.
    failedPages.clear();
    publish({ loadError: null });
    await Promise.all(offsets.map((offset) => loadPage(offset)));
  };

  const ensureRange = (startIndex: number, endIndex: number) => {
    activePages = new Set();
    if (state.total === 0 || endIndex < startIndex) return;
    const firstOffset = Math.floor(Math.max(0, startIndex) / pageSize) * pageSize;
    const lastOffset =
      Math.floor(Math.min(state.total - 1, Math.max(startIndex, endIndex)) / pageSize) * pageSize;
    for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
      if (activePages.size < MAX_CACHED_PAGES) activePages.add(pageOffset);
    }
    for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
      void loadPage(pageOffset).catch(() => {});
    }
  };

  const handleReachEnd = () => {
    if (state.loading || state.offset >= state.total) return;
    void loadPage(Math.floor(state.offset / pageSize) * pageSize).catch(() => {});
  };

  const getAssetAt = (index: number) => {
    visibleCache();
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    const assetId = state.cache.pages.get(pageOffset)?.[index - pageOffset];
    return assetId === undefined ? undefined : state.cache.assetsById.get(assetId);
  };

  const getAssetAtAsync = async (index: number) => {
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    const existing = getAssetAt(index);
    if (existing) {
      touchPage(pageOffset);
      return existing;
    }
    const page = await loadPage(pageOffset);
    return page?.[index - pageOffset];
  };

  const retryAssetAt = (index: number) => {
    // Explicit lightbox recovery unblocks only its target, including an offscreen page.
    failedPages.delete(Math.floor(index / pageSize) * pageSize);
    if (failedPages.size === 0) publish({ loadError: null });
    return getAssetAtAsync(index);
  };

  /** Ordered snapshot IDs for a global index range; loads only the pages that
   * intersect it. Rejects when any required page fails to load, so callers can
   * leave their state untouched instead of applying a partial range. */
  const getIdsRangeAsync = async (startIndex: number, endIndex: number) => {
    const generation = currentGeneration;
    const sessionId = currentSessionId;
    const from = Math.max(0, Math.min(startIndex, endIndex));
    const to = Math.min(state.total - 1, Math.max(startIndex, endIndex));
    if (from > to) return [];
    const ids: number[] = [];
    for (
      let pageOffset = Math.floor(from / pageSize) * pageSize;
      pageOffset <= to;
      pageOffset += pageSize
    ) {
      const page = await loadPage(pageOffset);
      if (generation !== currentGeneration || sessionId !== currentSessionId)
        throw new Error("Asset ID range was cancelled");
      if (!page) throw new Error("Asset ID range was cancelled");
      const firstLocal = Math.max(0, from - pageOffset);
      const lastLocal = Math.min(pageSize - 1, to - pageOffset);
      for (let local = firstLocal; local <= lastLocal; local += 1) {
        const summary = page[local];
        if (!summary) throw new Error("Asset ID range is incomplete");
        ids.push(summary.id);
      }
    }
    return ids;
  };

  const getAssetIndex = (assetId: number) => {
    if (currentSessionId === null || !state.cache.assetsById.has(assetId)) return null;
    for (const [pageOffset, ids] of state.cache.pages) {
      const localIndex = ids.indexOf(assetId);
      if (localIndex >= 0) return pageOffset + localIndex;
    }
    return null;
  };

  const patchAssets = (action: (assets: AssetSummary[]) => AssetSummary[]) => {
    currentPatchCounter += 1;
    const next = action(Array.from(state.cache.assetsById.values()));
    // Preserve snapshot positions until the next query replaces them.
    publish({
      cache: { ...state.cache, assetsById: new Map(next.map((asset) => [asset.id, asset])) }
    });
  };

  return {
    // Commands need the session immediately, before batched reactive publication.
    navigationSnapshot: () => ({
      epoch: state.queryEpoch,
      pending: state.queryPending,
      total: state.total
    }),
    get assets() {
      return assets();
    },
    get total() {
      return readTotal();
    },
    get offset() {
      return readOffset();
    },
    get loading() {
      return readLoading();
    },
    get loadError() {
      return readLoadError();
    },
    get pageFailureEpoch() {
      return readPageFailureEpoch();
    },
    get queryEpoch() {
      return readQueryEpoch();
    },
    get queryPending() {
      return readQueryPending();
    },
    patchAssets,
    retryLoad,
    getAssetPosition,
    refresh,
    reset,
    handleReachEnd,
    ensureRange,
    getAssetAt,
    getAssetAtAsync,
    retryAssetAt,
    getIdsRangeAsync,
    getAssetIndex
  };
}
