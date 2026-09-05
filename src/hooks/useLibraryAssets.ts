import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { getAssetQueryPage, startAssetQuery } from "../api";
import type { SearchMediaKind } from "../components/app/types";
import type { AssetSummary, SearchMetaFilter } from "../types";
import { mapThumbs } from "../utils/media";

const MAX_CACHED_PAGES = 12;

interface PageCache {
  pages: Map<number, number[]>;
  assetsById: Map<number, AssetSummary>;
  lru: number[];
}

interface UseLibraryAssetsOptions {
  pageSize: number;
  filterInclude: string[];
  filterExclude: string[];
  metaFilter?: SearchMetaFilter | null;
  appliedMediaKind: SearchMediaKind;
  appliedFavoritesOnly: boolean;
  setThumbs: Dispatch<SetStateAction<Record<number, string>>>;
  resetThumbnailQueue: () => void;
}

interface UseLibraryAssetsResult {
  assets: AssetSummary[];
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  total: number;
  setTotal: Dispatch<SetStateAction<number>>;
  offset: number;
  setOffset: Dispatch<SetStateAction<number>>;
  loading: boolean;
  setLoading: Dispatch<SetStateAction<boolean>>;
  loadError: string | null;
  retryLoad: () => Promise<void>;
  pageFailureEpoch: number;
  queryEpoch: number;
  refresh: () => Promise<void>;
  handleReachEnd: () => void;
  ensureRange: (startIndex: number, endIndex: number) => void;
  getAssetAt: (index: number) => AssetSummary | undefined;
  getAssetAtAsync: (index: number) => Promise<AssetSummary | undefined>;
  getIdsRangeAsync: (startIndex: number, endIndex: number) => Promise<number[]>;
  getAssetIndex: (assetId: number) => number | null;
}

const EMPTY_CACHE: PageCache = {
  pages: new Map(),
  assetsById: new Map(),
  lru: []
};

export function useLibraryAssets({
  pageSize,
  filterInclude,
  filterExclude,
  metaFilter = null,
  appliedMediaKind,
  appliedFavoritesOnly,
  setThumbs,
  resetThumbnailQueue
}: UseLibraryAssetsOptions): UseLibraryAssetsResult {
  const [cache, setCache] = useState<PageCache>(EMPTY_CACHE);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [pageFailureEpoch, setPageFailureEpoch] = useState(0);
  const [queryEpoch, setQueryEpoch] = useState(0);
  const sessionIdRef = useRef<number | null>(null);
  const generationRef = useRef(0);
  const inFlightPagesRef = useRef<Map<number, Promise<AssetSummary[] | undefined>>>(new Map());
  const inFlightCountRef = useRef(0);
  const activePagesRef = useRef(new Set<number>());
  const accessOrderRef = useRef<number[]>([]);
  const touchPage = useCallback((pageOffset: number) => {
    accessOrderRef.current = [pageOffset, ...accessOrderRef.current.filter((offset) => offset !== pageOffset)]
      .slice(0, MAX_CACHED_PAGES);
  }, []);

  const beginLoading = useCallback(() => {
    inFlightCountRef.current += 1;
    setLoading(true);
  }, []);
  const endLoading = useCallback(() => {
    inFlightCountRef.current = Math.max(0, inFlightCountRef.current - 1);
    setLoading(inFlightCountRef.current > 0);
  }, []);

  // Summaries are stored verbatim: no path/size/tags fabrication happens here.
  const mergePage = useCallback(
    (pageOffset: number, summaries: AssetSummary[], replace: boolean) => {
      touchPage(pageOffset);
      setCache((previous) => {
        const pages = replace ? new Map<number, number[]>() : new Map(previous.pages);
        const assetsById = replace ? new Map<number, AssetSummary>() : new Map(previous.assetsById);
        const pageIds = summaries.map((summary) => summary.id);
        pages.set(pageOffset, pageIds);
        for (const summary of summaries) {
          assetsById.set(summary.id, summary);
        }

        const lru = [...new Set([...accessOrderRef.current, pageOffset, ...(replace ? [] : previous.lru)])]
          .filter((offset) => pages.has(offset));
        while (lru.length > MAX_CACHED_PAGES) {
          let candidate = lru.length - 1;
          while (candidate >= 0 && activePagesRef.current.has(lru[candidate])) candidate--;
          const [evictedOffset] = lru.splice(candidate < 0 ? lru.length - 1 : candidate, 1);
          if (evictedOffset === undefined) break;
          const evictedIds = pages.get(evictedOffset) ?? [];
          pages.delete(evictedOffset);
          for (const assetId of evictedIds) {
            assetsById.delete(assetId);
          }
        }

        return { pages, assetsById, lru };
      });
      setThumbs((previous) => {
        const incoming = mapThumbs(summaries);
        return replace ? incoming : { ...previous, ...incoming };
      });
      setOffset((current) => Math.max(current, pageOffset + summaries.length));
    },
    [setThumbs, touchPage]
  );

  const refresh = useCallback(async () => {
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    sessionIdRef.current = null;
    inFlightPagesRef.current.clear();
    activePagesRef.current.clear();
    accessOrderRef.current = [];
    resetThumbnailQueue();
    beginLoading();
    const perfMark = `asset-query-${generation}`;
    if (import.meta.env.VITE_MEDIATAGGER_PERF === "1") performance.mark(`${perfMark}-start`);
    try {
      const result = await startAssetQuery({
        tagsAnd: filterInclude,
        tagsNot: filterExclude,
        mediaKind: appliedMediaKind,
        favoritesOnly: appliedFavoritesOnly,
        metaFilter,
        generation,
        pageSize
      });
      if (generation !== generationRef.current || result.status === "superseded") return;
      sessionIdRef.current = result.session_id;
      setQueryEpoch((epoch) => epoch + 1);
      setTotal(result.total);
      setOffset(result.items.length);
      setLoadError(null);
      mergePage(0, result.items, true);
    } catch (error) {
      if (generation === generationRef.current) {
        setLoadError(error instanceof Error ? error.message : String(error));
        setPageFailureEpoch((epoch) => epoch + 1);
      }
      throw error instanceof Error ? error : new Error(String(error));
    } finally {
      if (import.meta.env.VITE_MEDIATAGGER_PERF === "1") {
        performance.mark(`${perfMark}-end`);
        performance.measure("asset-query-refresh", `${perfMark}-start`, `${perfMark}-end`);
      }
      endLoading();
    }
  }, [
    appliedFavoritesOnly,
    appliedMediaKind,
    beginLoading,
    endLoading,
    filterExclude,
    filterInclude,
    mergePage,
    metaFilter,
    pageSize,
    resetThumbnailQueue
  ]);

  const retryLoad = useCallback(async () => {
    setLoadError(null);
    await refresh();
  }, [refresh]);

  const loadPage = useCallback(
    async (pageOffset: number) => {
      const sessionId = sessionIdRef.current;
      if (sessionId === null || pageOffset < 0 || pageOffset >= total) return undefined;
      touchPage(pageOffset);
      if (cache.pages.has(pageOffset)) {
        return (cache.pages.get(pageOffset) ?? [])
          .map((assetId) => cache.assetsById.get(assetId))
          .filter((summary): summary is AssetSummary => Boolean(summary));
      }
      const inFlight = inFlightPagesRef.current.get(pageOffset);
      if (inFlight) return inFlight;

      const generation = generationRef.current;
      let request!: Promise<AssetSummary[] | undefined>;
      request = (async () => {
        beginLoading();
        try {
          const result = await getAssetQueryPage(sessionId, pageOffset, pageSize);
          if (generation !== generationRef.current) return undefined;
          if (result.status === "stale") {
            await refresh();
            return undefined;
          }
          setLoadError(null);
          mergePage(result.offset, result.items, false);
          return result.items;
        } catch (error) {
          // The failed page stays a retryable hole; the range dedup marker is
          // released through pageFailureEpoch so the virtual range re-requests.
          if (generation === generationRef.current) {
            setLoadError(error instanceof Error ? error.message : String(error));
            setPageFailureEpoch((epoch) => epoch + 1);
          }
          throw error instanceof Error ? error : new Error(String(error));
        } finally {
          if (inFlightPagesRef.current.get(pageOffset) === request) {
            inFlightPagesRef.current.delete(pageOffset);
          }
          endLoading();
        }
      })();
      inFlightPagesRef.current.set(pageOffset, request);
      return request;
    }, [beginLoading, cache.pages, cache.assetsById, endLoading, mergePage, pageSize, refresh, total, touchPage]
  );

  const ensureRange = useCallback(
    (startIndex: number, endIndex: number) => {
      activePagesRef.current = new Set();
      if (total === 0 || endIndex < startIndex) return;
      const firstOffset = Math.floor(Math.max(0, startIndex) / pageSize) * pageSize;
      const lastOffset = Math.floor(Math.min(total - 1, Math.max(startIndex, endIndex)) / pageSize) * pageSize;
      for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
        if (activePagesRef.current.size < MAX_CACHED_PAGES) activePagesRef.current.add(pageOffset);
      }
      for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
        void loadPage(pageOffset).catch(() => {});
      }
    },
    [loadPage, pageSize, total]
  );

  const handleReachEnd = useCallback(() => {
    if (loading || offset >= total) return;
    void loadPage(Math.floor(offset / pageSize) * pageSize).catch(() => {});
  }, [loadPage, loading, offset, pageSize, total]);

  const getAssetAt = useCallback(
    (index: number) => {
      const pageOffset = Math.floor(index / pageSize) * pageSize;
      const assetId = cache.pages.get(pageOffset)?.[index - pageOffset];
      return assetId === undefined ? undefined : cache.assetsById.get(assetId);
    },
    [cache, pageSize]
  );

  const getAssetAtAsync = useCallback(
    async (index: number) => {
      const pageOffset = Math.floor(index / pageSize) * pageSize;
      const existing = getAssetAt(index);
      if (existing) {
        touchPage(pageOffset);
        return existing;
      }
      const page = await loadPage(pageOffset);
      return page?.[index - pageOffset];
    },
    [getAssetAt, loadPage, pageSize, touchPage]
  );

  /** Ordered snapshot IDs for a global index range; loads only the pages that
   * intersect it. Rejects when any required page fails to load, so callers can
   * leave their state untouched instead of applying a partial range. */
  const getIdsRangeAsync = useCallback(
    async (startIndex: number, endIndex: number) => {
      const from = Math.max(0, Math.min(startIndex, endIndex));
      const to = Math.min(total - 1, Math.max(startIndex, endIndex));
      if (from > to) return [];
      const ids: number[] = [];
      for (
        let pageOffset = Math.floor(from / pageSize) * pageSize;
        pageOffset <= to;
        pageOffset += pageSize
      ) {
        const page = await loadPage(pageOffset);
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
    },
    [loadPage, pageSize, total]
  );

  const getAssetIndex = useCallback(
    (assetId: number) => {
      for (const [pageOffset, ids] of cache.pages) {
        const localIndex = ids.indexOf(assetId);
        if (localIndex >= 0) return pageOffset + localIndex;
      }
      return null;
    },
    [cache.pages]
  );

  const assets = useMemo(() => Array.from(cache.assetsById.values()), [cache.assetsById]);

  useEffect(() => {
    const activeIds = new Set(cache.assetsById.keys());
    setThumbs((current) => {
      const next = { ...current };
      let changed = false;
      for (const assetIdText of Object.keys(next)) {
        const assetId = Number(assetIdText);
        if (!activeIds.has(assetId)) {
          delete next[assetId];
          changed = true;
        }
      }
      return changed ? next : current;
    });
  }, [cache.assetsById, setThumbs]);

  const setAssets = useCallback<Dispatch<SetStateAction<AssetSummary[]>>>((action) => {
    setCache((previous) => {
      const current = Array.from(previous.assetsById.values());
      const nextAssets = typeof action === "function" ? action(current) : action;
      const assetsById = new Map(nextAssets.map((asset) => [asset.id, asset]));
      const pages = new Map<number, number[]>();
      for (const [pageOffset, ids] of previous.pages) {
        pages.set(pageOffset, ids.filter((assetId) => assetsById.has(assetId)));
      }
      return { ...previous, pages, assetsById };
    });
  }, []);

  return {
    assets,
    setAssets,
    total,
    setTotal,
    offset,
    setOffset,
    loading,
    setLoading,
    loadError,
    retryLoad,
    pageFailureEpoch,
    queryEpoch,
    refresh,
    handleReachEnd,
    ensureRange,
    getAssetAt,
    getAssetAtAsync,
    getIdsRangeAsync,
    getAssetIndex
  };
}
