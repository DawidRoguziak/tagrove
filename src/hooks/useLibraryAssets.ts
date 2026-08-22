import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { getAssetQueryPage, startAssetQuery } from "../api";
import type { SearchMediaKind } from "../components/app/types";
import type { Asset, AssetSummary, SearchMetaFilter } from "../types";
import { mapThumbs } from "../utils/media";

const MAX_CACHED_PAGES = 12;

interface PageCache {
  pages: Map<number, number[]>;
  assetsById: Map<number, Asset>;
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
  assets: Asset[];
  setAssets: Dispatch<SetStateAction<Asset[]>>;
  total: number;
  setTotal: Dispatch<SetStateAction<number>>;
  offset: number;
  setOffset: Dispatch<SetStateAction<number>>;
  loading: boolean;
  setLoading: Dispatch<SetStateAction<boolean>>;
  loadError: string | null;
  retryLoad: () => Promise<void>;
  pageFailureEpoch: number;
  refresh: () => Promise<void>;
  handleReachEnd: () => void;
  ensureRange: (startIndex: number, endIndex: number) => void;
  getAssetAt: (index: number) => Asset | undefined;
  getAssetAtAsync: (index: number) => Promise<Asset | undefined>;
  getAssetIndex: (assetId: number) => number | null;
}

const EMPTY_CACHE: PageCache = {
  pages: new Map(),
  assetsById: new Map(),
  lru: []
};

function summaryToAsset(summary: AssetSummary): Asset {
  return {
    id: summary.id,
    path: summary.preview_path ?? summary.file_name,
    kind: summary.kind,
    size_bytes: 0,
    modified_at: summary.modified_at,
    width: summary.width,
    height: summary.height,
    duration_ms: summary.duration_ms,
    thumb_path: summary.thumb_path,
    is_favorite: summary.is_favorite,
    media_group_key: summary.media_group_key,
    media_group_order: summary.media_group_order,
    tags: []
  };
}

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
  const sessionIdRef = useRef<number | null>(null);
  const generationRef = useRef(0);
  const inFlightPagesRef = useRef<Map<number, Promise<Asset[] | undefined>>>(new Map());
  const inFlightCountRef = useRef(0);

  const beginLoading = useCallback(() => {
    inFlightCountRef.current += 1;
    setLoading(true);
  }, []);
  const endLoading = useCallback(() => {
    inFlightCountRef.current = Math.max(0, inFlightCountRef.current - 1);
    setLoading(inFlightCountRef.current > 0);
  }, []);

  const mergePage = useCallback(
    (pageOffset: number, summaries: AssetSummary[], replace: boolean) => {
      const pageAssets = summaries.map(summaryToAsset);
      setCache((previous) => {
        const pages = replace ? new Map<number, number[]>() : new Map(previous.pages);
        const assetsById = replace ? new Map<number, Asset>() : new Map(previous.assetsById);
        const pageIds = pageAssets.map((asset) => asset.id);
        pages.set(pageOffset, pageIds);
        for (const asset of pageAssets) {
          assetsById.set(asset.id, asset);
        }

        const lru = [pageOffset, ...(replace ? [] : previous.lru.filter((item) => item !== pageOffset))];
        while (lru.length > MAX_CACHED_PAGES) {
          const evictedOffset = lru.pop();
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
        const incoming = mapThumbs(pageAssets);
        return replace ? incoming : { ...previous, ...incoming };
      });
      setOffset((current) => Math.max(current, pageOffset + pageAssets.length));
    },
    [setThumbs]
  );

  const refresh = useCallback(async () => {
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    sessionIdRef.current = null;
    inFlightPagesRef.current.clear();
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
      if (cache.pages.has(pageOffset)) {
        return (cache.pages.get(pageOffset) ?? [])
          .map((assetId) => cache.assetsById.get(assetId))
          .filter((asset): asset is Asset => Boolean(asset));
      }
      const inFlight = inFlightPagesRef.current.get(pageOffset);
      if (inFlight) return inFlight;

      const generation = generationRef.current;
      let request!: Promise<Asset[] | undefined>;
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
          return result.items.map(summaryToAsset);
        } catch (error) {
          // The failed page stays a retryable hole; the range dedup marker is
          // released through pageFailureEpoch so the virtual range re-requests.
          if (generation === generationRef.current) {
            setLoadError(error instanceof Error ? error.message : String(error));
          }
          setPageFailureEpoch((epoch) => epoch + 1);
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
    }, [beginLoading, cache.pages, endLoading, mergePage, pageSize, refresh, total]
  );

  const ensureRange = useCallback(
    (startIndex: number, endIndex: number) => {
      if (total === 0) return;
      const firstOffset = Math.floor(Math.max(0, startIndex) / pageSize) * pageSize;
      const lastOffset = Math.floor(Math.min(total - 1, Math.max(startIndex, endIndex)) / pageSize) * pageSize;
      for (let pageOffset = firstOffset; pageOffset <= lastOffset; pageOffset += pageSize) {
        void loadPage(pageOffset);
      }
    },
    [loadPage, pageSize, total]
  );

  const handleReachEnd = useCallback(() => {
    if (loading || offset >= total) return;
    void loadPage(Math.floor(offset / pageSize) * pageSize);
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
      const existing = getAssetAt(index);
      if (existing) return existing;
      const pageOffset = Math.floor(index / pageSize) * pageSize;
      const page = await loadPage(pageOffset);
      return page?.[index - pageOffset];
    },
    [getAssetAt, loadPage, pageSize]
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

  const setAssets = useCallback<Dispatch<SetStateAction<Asset[]>>>((action) => {
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
    refresh,
    handleReachEnd,
    ensureRange,
    getAssetAt,
    getAssetAtAsync,
    getAssetIndex
  };
}
