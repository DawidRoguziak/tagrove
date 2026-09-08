import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { listTags } from "../../../api";
import type { TagListPage } from "../../../types";
import {
  createFallbackPage,
  dedupeKnownTags,
  mergeTagPages
} from "../services/tagListSelectionService";

interface UseTagListDataOptions {
  open: boolean;
  knownTags: string[];
  query: string;
  listRef: RefObject<HTMLDivElement | null>;
  pageSize: number;
  loadMoreThresholdPx: number;
}

export function useTagListData({
  open,
  knownTags,
  query,
  listRef,
  pageSize,
  loadMoreThresholdPx
}: UseTagListDataOptions) {
  const requestIdRef = useRef(0);
  const [page, setPage] = useState<TagListPage>({ items: [], total: 0 });
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const fallbackKnownTags = useMemo(
    () => open ? dedupeKnownTags(knownTags) : [],
    [open, knownTags]
  );

  const resetPage = useCallback(() => {
    requestIdRef.current += 1;
    setPage(createFallbackPage(fallbackKnownTags, "", 0, pageSize));
    setLoading(false);
    setLoadingMore(false);
  }, [fallbackKnownTags, pageSize]);

  useEffect(() => {
    if (!open) {
      resetPage();
      return;
    }

    const currentRequestId = requestIdRef.current + 1;
    const fallbackPage = createFallbackPage(fallbackKnownTags, query, 0, pageSize);
    requestIdRef.current = currentRequestId;

    setPage(fallbackPage);
    setLoading(true);
    setLoadingMore(false);

    if (listRef.current) {
      listRef.current.scrollTop = 0;
    }

    void listTags({ query, offset: 0, limit: pageSize })
      .then((nextPage) => {
        if (requestIdRef.current !== currentRequestId) {
          return;
        }

        setPage(nextPage);
      })
      .catch(() => {
        if (requestIdRef.current !== currentRequestId) {
          return;
        }

        setPage(fallbackPage);
      })
      .finally(() => {
        if (requestIdRef.current !== currentRequestId) {
          return;
        }

        setLoading(false);
        setLoadingMore(false);
      });
  }, [fallbackKnownTags, listRef, open, pageSize, query, resetPage]);

  const loadMore = useCallback(async () => {
    if (!open || loading || loadingMore || page.items.length >= page.total) {
      return;
    }

    const currentRequestId = requestIdRef.current;
    const nextOffset = page.items.length;
    const fallbackPage = createFallbackPage(fallbackKnownTags, query, nextOffset, pageSize);

    setLoadingMore(true);

    try {
      const nextPage = await listTags({
        query,
        offset: nextOffset,
        limit: pageSize
      });

      if (requestIdRef.current !== currentRequestId) {
        return;
      }

      setPage((previousPage) => ({
        items: mergeTagPages(previousPage.items, nextPage.items),
        total: nextPage.total
      }));
    } catch {
      if (requestIdRef.current !== currentRequestId) {
        return;
      }

      setPage((previousPage) => ({
        items: mergeTagPages(previousPage.items, fallbackPage.items),
        total: Math.max(previousPage.total, fallbackPage.total)
      }));
    } finally {
      if (requestIdRef.current === currentRequestId) {
        setLoadingMore(false);
      }
    }
  }, [fallbackKnownTags, loading, loadingMore, open, page.items.length, page.total, pageSize, query]);

  useEffect(() => {
    if (!open || loading || loadingMore || page.items.length === 0 || page.items.length >= page.total) {
      return;
    }

    const listNode = listRef.current;
    if (!listNode || listNode.scrollHeight > listNode.clientHeight + loadMoreThresholdPx) {
      return;
    }

    void loadMore();
  }, [listRef, loadMore, loading, loadingMore, open, page.items.length, page.total, loadMoreThresholdPx]);

  return {
    page,
    loading,
    loadingMore,
    resetPage,
    loadMore
  };
}
