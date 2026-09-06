import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { AssetSummary } from "../../../types";

const TILE_GAP = 10;
const getRowKey = (index: number) => index;

export interface GalleryRange {
  startIndex: number;
  endIndex: number;
  visibleStartIndex: number;
  visibleEndIndex: number;
}

interface UseGalleryVirtualGridOptions {
  assetCount: number;
  getAssetAt: (index: number) => AssetSummary | undefined;
  tileSize: number;
  hasMore: boolean;
  isLoading: boolean;
  galleryRef: RefObject<HTMLElement | null>;
  gridRef: RefObject<HTMLDivElement | null>;
  scrollContainerRef?: RefObject<HTMLElement | null>;
  onReachEnd: () => void;
  onVirtualRangeChange?: (range: GalleryRange) => void;
  rangeResetKey?: number;
}

export function useGalleryVirtualGrid({
  assetCount,
  getAssetAt,
  tileSize,
  hasMore,
  isLoading,
  galleryRef,
  gridRef,
  scrollContainerRef,
  onReachEnd,
  onVirtualRangeChange,
  rangeResetKey = 0
}: UseGalleryVirtualGridOptions) {
  const [geometry, setGeometry] = useState({ width: 0, margin: 0 });
  const getScrollElement = useCallback(
    // A supplied parent ref can still be null during the child's mount.
    () => scrollContainerRef ? scrollContainerRef.current : galleryRef.current,
    [scrollContainerRef, galleryRef]
  );

  // Parent DOM refs are attached before passive effects run.
  useEffect(() => {
    const gallery = galleryRef.current;
    const grid = gridRef.current;
    const scroller = getScrollElement();
    if (!gallery || !grid || !scroller) return;
    const measure = () => {
      const width = grid.clientWidth;
      const margin =
        grid.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top -
        scroller.clientTop +
        scroller.scrollTop;
      setGeometry((current) =>
        current.width === width && current.margin === margin ? current : { width, margin }
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(gallery);
    observer.observe(scroller);
    observer.observe(grid);
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [galleryRef, gridRef, getScrollElement, assetCount === 0]);

  const columnCount = Math.max(1, Math.floor((geometry.width + TILE_GAP) / (tileSize + TILE_GAP)));
  const tilePixelSize = Math.max(
    1,
    Math.floor((Math.max(geometry.width, tileSize) - TILE_GAP * (columnCount - 1)) / columnCount)
  );
  const estimateSize = useCallback(() => tilePixelSize, [tilePixelSize]);
  const rowVirtualizer = useVirtualizer({
    count: Math.ceil(assetCount / columnCount),
    getItemKey: getRowKey,
    getScrollElement,
    estimateSize,
    overscan: 4,
    gap: TILE_GAP,
    scrollMargin: geometry.margin
  });

  const previousGeometry = useRef({ columnCount, tilePixelSize, margin: geometry.margin });
  useLayoutEffect(() => {
    const previous = previousGeometry.current;
    previousGeometry.current = { columnCount, tilePixelSize, margin: geometry.margin };
    if (previous.columnCount === columnCount && previous.tilePixelSize === tilePixelSize) return;
    const scrollTop = getScrollElement()?.scrollTop ?? 0;
    const firstIndex =
      Math.floor(Math.max(0, scrollTop - previous.margin) / (previous.tilePixelSize + TILE_GAP)) *
      previous.columnCount;
    rowVirtualizer.measure();
    if (scrollTop > previous.margin) {
      rowVirtualizer.scrollToOffset(
        geometry.margin + Math.floor(firstIndex / columnCount) * (tilePixelSize + TILE_GAP)
      );
    }
  }, [columnCount, tilePixelSize, geometry.margin, getScrollElement, rowVirtualizer]);

  const rows = rowVirtualizer.getVirtualItems();
  const virtualItems = useMemo(
    () =>
      rows.flatMap((row) => {
        const first = row.index * columnCount;
        return Array.from({ length: Math.min(columnCount, assetCount - first) }, (_, lane) => ({
          index: first + lane,
          lane,
          start: row.start - geometry.margin
        }));
      }),
    [rows, columnCount, assetCount, geometry.margin]
  );
  const visibleStartIndex = (rowVirtualizer.range?.startIndex ?? 0) * columnCount;
  const visibleEndIndex = Math.min(
    assetCount - 1,
    ((rowVirtualizer.range?.endIndex ?? -1) + 1) * columnCount - 1
  );
  const startIndex = virtualItems[0]?.index ?? 0;
  const endIndex = virtualItems[virtualItems.length - 1]?.index ?? -1;

  useEffect(() => {
    onVirtualRangeChange?.({ startIndex, endIndex, visibleStartIndex, visibleEndIndex });
  }, [
    onVirtualRangeChange,
    startIndex,
    endIndex,
    visibleStartIndex,
    visibleEndIndex,
    getAssetAt,
    rangeResetKey
  ]);

  useEffect(() => {
    if (hasMore && !isLoading && assetCount > 0 && endIndex >= assetCount - columnCount * 2) {
      onReachEnd();
    }
  }, [assetCount, columnCount, endIndex, hasMore, isLoading, onReachEnd]);

  return {
    tileGap: TILE_GAP,
    tilePixelSize,
    columnCount,
    virtualItems,
    totalSize: rowVirtualizer.getTotalSize()
  };
}
