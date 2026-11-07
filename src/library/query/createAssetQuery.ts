import { createMemo, createSignal, onCleanup } from "solid-js";
import {
  getAssetQueryIdRange,
  getAssetQueryPage,
  getAssetQueryPosition,
  getAssetQuerySections,
  startAssetQuery
} from "../../platform/tauri/assets";
import type {
  AssetQueryPositionResult,
  AssetQuerySection,
  AssetSummary,
  SearchMetaFilter,
  MediaKind
} from "../../platform/tauri/contracts/assets";

export const MAX_CACHED_PAGES = 24;
/** A `stale` read schedules one refresh after this delay; later stale reads join it. */
export const STALE_REFRESH_DELAY_MS = 250;
/** Minimum spacing between two stale-driven refreshes. */
export const STALE_REFRESH_MIN_INTERVAL_MS = 1000;
/** Largest ID range requested in one binary IPC call (8 MB of f64 values). */
export const ID_RANGE_CHUNK = 1_000_000;

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
  /** First visible global index; a refresh asks the backend for that page first. */
  anchorIndex?: () => number | null | undefined;
  onQueryChanged?: () => void;
}

export function createAssetQuery({
  pageSize = 128,
  filters,
  anchorIndex,
  onQueryChanged
}: Options) {
  const ownerId = crypto.randomUUID();
  let state = {
    total: 0,
    loading: false,
    loadError: null as string | null,
    pageFailureEpoch: 0,
    queryPending: false,
    queryEpoch: 0
  };
  const [snapshot, setSnapshot] = createSignal(state);
  // The page cache is mutated in place; `version` is the only reactive notification.
  // Every render-time read (`getAssetAt`, `assets`) must read it to stay subscribed.
  const pages = new Map<number, number[]>();
  const assetsById = new Map<number, AssetSummary>();
  let lru: number[] = [];
  // Rows of the previous query's viewport pages, shown until their offset reloads.
  // Render-only: indexes, async reads, ranges and `assets` never see them.
  const fallback = new Map<number, AssetSummary[]>();
  const [version, setVersion] = createSignal(0);
  const assets = createMemo(() => {
    version();
    return Array.from(assetsById.values());
  });
  const readTotal = createMemo(() => snapshot().total);
  const readLoading = createMemo(() => snapshot().loading);
  const readLoadError = createMemo(() => snapshot().loadError);
  const readPageFailureEpoch = createMemo(() => snapshot().pageFailureEpoch);
  const readQueryEpoch = createMemo(() => snapshot().queryEpoch);
  const readQueryPending = createMemo(() => snapshot().queryPending);
  // Timeline sections of the current session (stage 08); empty until loaded.
  const [sections, setSections] = createSignal<{
    sessionId: number | null;
    items: readonly AssetQuerySection[];
  }>({ sessionId: null, items: [] });
  let disposed = false;
  function publish(patch: Partial<typeof state>) {
    state = { ...state, ...patch };
    setSnapshot(state);
    if (patch.queryEpoch !== undefined) onQueryChanged?.();
  }
  function cacheChanged() {
    setVersion((value) => value + 1);
  }
  let staleTimer: ReturnType<typeof setTimeout> | null = null;
  let lastStaleRefreshAt = Number.NEGATIVE_INFINITY;
  const cancelStaleRefresh = () => {
    if (staleTimer !== null) clearTimeout(staleTimer);
    staleTimer = null;
  };
  let currentSessionId: number | null = null;
  let currentGeneration = 0;
  let currentPatchCounter = 0;
  const inFlightPages: Map<number, Promise<AssetSummary[] | undefined>> = new Map();
  const failedPages = new Map<number, Error>();
  let inFlightCount = 0;
  let activePages = new Set<number>();
  // Pages of the latest gallery range; unlike `activePages` it survives a refresh so
  // the new query knows which old rows may stay on screen as fallback.
  let viewportPages = new Set<number>();
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
    cancelStaleRefresh();
    invalidateReads();
  });
  const clearCache = () => {
    pages.clear();
    assetsById.clear();
    lru = [];
  };
  const reset = () => {
    cancelStaleRefresh();
    invalidateReads();
    activePages.clear();
    viewportPages.clear();
    accessOrder = [];
    clearCache();
    fallback.clear();
    setSections({ sessionId: null, items: [] });
    cacheChanged();
    publish({
      total: 0,
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
  // The cache is mutated in place; one `version` bump publishes the change.
  const mergePage = (pageOffset: number, summaries: AssetSummary[], replace: boolean) => {
    touchPage(pageOffset);
    if (replace) {
      fallback.clear();
      // Keep the old viewport visible until each of its pages reloads under the new query.
      for (const offset of viewportPages) {
        const ids = pages.get(offset);
        if (offset === pageOffset || offset >= state.total || !ids) continue;
        const rows = ids.map((id) => assetsById.get(id));
        if (rows.every(Boolean)) fallback.set(offset, rows as AssetSummary[]);
      }
      clearCache();
    } else {
      fallback.delete(pageOffset);
      const previous = pages.get(pageOffset);
      if (previous) for (const id of previous) assetsById.delete(id);
    }
    pages.set(
      pageOffset,
      summaries.map((summary) => summary.id)
    );
    for (const summary of summaries) assetsById.set(summary.id, summary);
    lru = [...new Set([...accessOrder, pageOffset, ...lru])].filter((offset) => pages.has(offset));
    while (lru.length > MAX_CACHED_PAGES) {
      let candidate = lru.length - 1;
      while (candidate >= 0 && activePages.has(lru[candidate])) candidate--;
      const [evictedOffset] = lru.splice(candidate < 0 ? lru.length - 1 : candidate, 1);
      if (evictedOffset === undefined) break;
      const evictedIds = pages.get(evictedOffset) ?? [];
      pages.delete(evictedOffset);
      for (const id of evictedIds) assetsById.delete(id);
    }
    cacheChanged();
  };

  const scheduleStaleRefresh = () => {
    // Stale reads join one pending refresh: at most one per STALE_REFRESH_MIN_INTERVAL_MS.
    if (disposed || staleTimer !== null) return;
    const wait = Math.max(
      STALE_REFRESH_DELAY_MS,
      lastStaleRefreshAt + STALE_REFRESH_MIN_INTERVAL_MS - Date.now()
    );
    staleTimer = setTimeout(() => {
      staleTimer = null;
      lastStaleRefreshAt = Date.now();
      void refresh().catch(() => {});
    }, wait);
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
    cancelStaleRefresh();
    invalidateReads();
    publish({ queryPending: true });
    publish({ queryEpoch: state.queryEpoch + 1 });
    const generation = currentGeneration;
    currentSessionId = null;
    inFlightPages.clear();
    activePages.clear();
    accessOrder = [];
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
        pageSize,
        anchorOffset: anchorIndex?.() ?? null
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
        scheduleStaleRefresh();
        return;
      }
      currentSessionId = result.session_id;
      if (sections().sessionId !== result.session_id) {
        setSections({ sessionId: null, items: [] });
        void loadSections(result.session_id, generation);
      }
      publish({ queryEpoch: state.queryEpoch + 1 });
      publish({ total: result.total });
      publish({ loadError: null });
      mergePage(result.offset, result.items, true);
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

  /** Sections are optional UI: a failure leaves the timeline hidden. */
  const loadSections = async (sessionId: number, generation: number) => {
    try {
      const result = await getAssetQuerySections(sessionId);
      if (generation !== currentGeneration || sessionId !== currentSessionId) return;
      if (result.status === "stale") {
        scheduleStaleRefresh();
        return;
      }
      setSections({ sessionId, items: result.total === state.total ? result.sections : [] });
    } catch {
      // Keep the timeline hidden; pages and selection do not depend on it.
    }
  };

  const loadPage = async (pageOffset: number) => {
    const sessionId = currentSessionId;
    if (sessionId === null || pageOffset < 0 || pageOffset >= state.total) return undefined;
    touchPage(pageOffset);
    const cached = pages.get(pageOffset);
    if (cached) {
      if (cached.some((id) => !assetsById.has(id))) return undefined;
      return cached
        .map((assetId) => assetsById.get(assetId))
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
          scheduleStaleRefresh();
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
    if (result.status === "stale") scheduleStaleRefresh();
    return result;
  };

  const retryLoad = async () => {
    if (currentSessionId === null) {
      await refresh();
      return;
    }
    const offsets = [...failedPages.keys()].filter((offset) => activePages.has(offset));
    // Explicit Retry also unblocks failed offscreen pages (lightbox reads).
    // Only viewport demand is fetched here; selections retry their own ID ranges.
    failedPages.clear();
    publish({ loadError: null });
    await Promise.all(offsets.map((offset) => loadPage(offset)));
  };

  const ensureRange = (startIndex: number, endIndex: number) => {
    activePages = new Set();
    viewportPages = new Set();
    if (state.total === 0 || endIndex < startIndex) {
      if (fallback.size) {
        fallback.clear();
        cacheChanged();
      }
      return;
    }
    const firstOffset = Math.floor(Math.max(0, startIndex) / pageSize) * pageSize;
    const lastOffset =
      Math.floor(Math.min(state.total - 1, Math.max(startIndex, endIndex)) / pageSize) * pageSize;
    for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
      viewportPages.add(pageOffset);
      if (activePages.size < MAX_CACHED_PAGES) activePages.add(pageOffset);
    }
    let droppedFallback = false;
    for (const offset of fallback.keys()) {
      if (viewportPages.has(offset)) continue;
      fallback.delete(offset);
      droppedFallback = true;
    }
    if (droppedFallback) cacheChanged();
    for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
      void loadPage(pageOffset).catch(() => {});
    }
  };

  /** Current-query rows only: never returns refresh fallback rows. */
  const readCached = (index: number) => {
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    const assetId = pages.get(pageOffset)?.[index - pageOffset];
    return assetId === undefined ? undefined : assetsById.get(assetId);
  };

  /** Render-time read: tracks `version` and may return a previous-query fallback row. */
  const getAssetAt = (index: number) => {
    version();
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    if (pages.has(pageOffset)) return readCached(index);
    return fallback.get(pageOffset)?.[index - pageOffset];
  };

  const getAssetAtAsync = async (index: number) => {
    const pageOffset = Math.floor(index / pageSize) * pageSize;
    const existing = readCached(index);
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

  /** Ordered snapshot IDs for an inclusive global index range, read from the
   * backend session vector through the binary range command. It never touches
   * the page cache. Rejects the whole request when the session is stale or
   * replaced, or when any position is missing, so callers can leave their
   * state untouched instead of applying a partial range. */
  const getIdsRangeAsync = async (startIndex: number, endIndex: number) => {
    const generation = currentGeneration;
    const sessionId = currentSessionId;
    const from = Math.max(0, Math.min(startIndex, endIndex));
    const to = Math.min(state.total - 1, Math.max(startIndex, endIndex));
    if (from > to) return new Float64Array(0);
    if (sessionId === null) throw new Error("Asset ID range was cancelled");
    const ids = new Float64Array(to - from + 1);
    for (let chunkStart = from; chunkStart <= to; chunkStart += ID_RANGE_CHUNK) {
      const chunkEnd = Math.min(to, chunkStart + ID_RANGE_CHUNK - 1);
      const result = await getAssetQueryIdRange(sessionId, chunkStart, chunkEnd);
      if (generation !== currentGeneration || sessionId !== currentSessionId)
        throw new Error("Asset ID range was cancelled");
      if (result.status === "stale") {
        scheduleStaleRefresh();
        throw new Error("Asset ID range was cancelled");
      }
      if (result.ids.length !== chunkEnd - chunkStart + 1)
        throw new Error("Asset ID range is incomplete");
      ids.set(result.ids, chunkStart - from);
    }
    return ids;
  };

  const getAssetIndex = (assetId: number) => {
    if (currentSessionId === null || !assetsById.has(assetId)) return null;
    for (const [pageOffset, ids] of pages) {
      const localIndex = ids.indexOf(assetId);
      if (localIndex >= 0) return pageOffset + localIndex;
    }
    return null;
  };

  const patchAssets = (action: (assets: AssetSummary[]) => AssetSummary[]) => {
    currentPatchCounter += 1;
    const next = action(Array.from(assetsById.values()));
    // Preserve snapshot positions until the next query replaces them.
    assetsById.clear();
    for (const asset of next) assetsById.set(asset.id, asset);
    cacheChanged();
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
    /** Timeline sections of the current session, in session order. */
    get sections() {
      return sections().items;
    },
    /** Current backend session, or `null` while a query is pending. */
    sessionId: () => currentSessionId,
    patchAssets,
    retryLoad,
    getAssetPosition,
    refresh,
    reset,
    ensureRange,
    getAssetAt,
    getAssetAtAsync,
    retryAssetAt,
    getIdsRangeAsync,
    getAssetIndex
  };
}
