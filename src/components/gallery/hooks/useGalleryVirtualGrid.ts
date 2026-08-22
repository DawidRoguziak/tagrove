import { useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Asset } from "../../../types";

const TILE_GAP = 10;
const GIF_VIEWPORT_ANIMATE_THRESHOLD = 10;

interface UseGalleryVirtualGridOptions {
  assetCount: number;
  getAssetAt: (index: number) => Asset | undefined;
  tileSize: number;
  hasMore: boolean;
  isLoading: boolean;
  galleryRef: RefObject<HTMLDivElement | null>;
  scrollContainerRef?: RefObject<HTMLElement | null>;
  onReachEnd: () => void;
  onVirtualRangeChange?: (startIndex: number, endIndex: number) => void;
  rangeResetKey?: number;
}

export function useGalleryVirtualGrid({
  assetCount,
  getAssetAt,
  tileSize,
  hasMore,
  isLoading,
  galleryRef,
  scrollContainerRef,
  onReachEnd,
  onVirtualRangeChange,
  rangeResetKey = 0
}: UseGalleryVirtualGridOptions) {
  const [galleryWidth, setGalleryWidth] = useState(0);
  const [galleryHeight, setGalleryHeight] = useState(0);
  const lastPrefetchRangeRef = useRef<{ start: number; end: number } | null>(null);

  useEffect(() => {
    const node = galleryRef.current;
    if (!node) {
      return;
    }

    setGalleryWidth(Math.round(node.clientWidth));
    setGalleryHeight(Math.round(node.clientHeight));
    const observer = new ResizeObserver((entries) => {
      const width = Math.round(entries[0]?.contentRect.width ?? 0);
      const height = Math.round(entries[0]?.contentRect.height ?? 0);
      setGalleryWidth((current) => (current === width ? current : width));
      setGalleryHeight((current) => (current === height ? current : height));
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [galleryRef]);

  const columnCount = Math.max(1, Math.floor((galleryWidth + TILE_GAP) / (tileSize + TILE_GAP)));
  const tilePixelSize =
    columnCount > 0
      ? Math.floor((Math.max(galleryWidth, tileSize) - TILE_GAP * (columnCount - 1)) / columnCount)
      : tileSize;
  const activeScrollElement = scrollContainerRef?.current ?? galleryRef.current;
  const usesExternalScroll = Boolean(
    scrollContainerRef?.current && scrollContainerRef.current !== galleryRef.current
  );
  const galleryOffsetTop = usesExternalScroll ? (galleryRef.current?.offsetTop ?? 0) : 0;

  const itemVirtualizer = useVirtualizer({
    count: assetCount,
    lanes: columnCount,
    getItemKey: (index) => getAssetAt(index)?.id ?? `pending-${index}`,
    getScrollElement: () => activeScrollElement,
    estimateSize: () => tilePixelSize,
    overscan: Math.max(columnCount * 4, 12),
    gap: TILE_GAP,
    isScrollingResetDelay: 120
  });

  const virtualItems = itemVirtualizer.getVirtualItems();
  const scrollTop = activeScrollElement?.scrollTop ?? 0;
  const viewportTop = Math.max(0, scrollTop - galleryOffsetTop);
  const viewportBottom = viewportTop + galleryHeight;

  const { visibleGifCount, itemsInViewport } = useMemo(() => {
    const visibleItems = new Set<number>();
    let gifCount = 0;

    for (const item of virtualItems) {
      const itemEnd = typeof item.end === "number" ? item.end : item.start + tilePixelSize;
      const isInViewport = itemEnd > viewportTop && item.start < viewportBottom;
      if (!isInViewport) {
        continue;
      }

      visibleItems.add(item.index);
      if (getAssetAt(item.index)?.kind === "gif") {
        gifCount += 1;
      }
    }

    return {
      visibleGifCount: gifCount,
      itemsInViewport: visibleItems
    };
  }, [getAssetAt, tilePixelSize, virtualItems, viewportBottom, viewportTop]);

  // The dedup marker must not treat a range as permanently handled while any
  // intersecting page request failed; rangeResetKey re-opens it for retries.
  useEffect(() => {
    lastPrefetchRangeRef.current = null;
  }, [assetCount, getAssetAt, rangeResetKey]);

  useEffect(() => {
    if (!onVirtualRangeChange || !virtualItems.length) {
      return;
    }

    let startIndex = Number.POSITIVE_INFINITY;
    let endIndex = -1;
    for (const item of virtualItems) {
      if (item.index < startIndex) {
        startIndex = item.index;
      }
      if (item.index > endIndex) {
        endIndex = item.index;
      }
    }

    const previousRange = lastPrefetchRangeRef.current;
    if (previousRange?.start === startIndex && previousRange.end === endIndex) {
      return;
    }

    lastPrefetchRangeRef.current = {
      start: startIndex,
      end: endIndex
    };
    onVirtualRangeChange(startIndex, endIndex);
  }, [onVirtualRangeChange, virtualItems]);

  useEffect(() => {
    if (!hasMore || isLoading || !assetCount || !virtualItems.length) {
      return;
    }

    const preloadThreshold = Math.max(columnCount * 2, 1);
    const lastVirtualIndex = virtualItems.reduce((maxIndex, item) => {
      return item.index > maxIndex ? item.index : maxIndex;
    }, -1);

    if (lastVirtualIndex >= assetCount - preloadThreshold) {
      onReachEnd();
    }
  }, [assetCount, columnCount, hasMore, isLoading, onReachEnd, virtualItems]);

  return {
    tileGap: TILE_GAP,
    tilePixelSize,
    columnCount,
    itemVirtualizer,
    virtualItems,
    visibleGifCount,
    itemsInViewport,
    shouldAnimateGif: (asset: Asset, itemIndex: number) => {
      return (
        asset.kind === "gif" &&
        !itemVirtualizer.isScrolling &&
        itemsInViewport.has(itemIndex) &&
        visibleGifCount <= GIF_VIEWPORT_ANIMATE_THRESHOLD
      );
    }
  };
}
