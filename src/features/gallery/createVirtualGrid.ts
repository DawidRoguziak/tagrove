import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import {
  Virtualizer,
  elementScroll,
  observeElementOffset,
  observeElementRect
} from "@tanstack/virtual-core";
import type { LibraryRange } from "../../library/createLibrary";

const GAP = 12;
const rowKey = (index: number) => index;
export function createVirtualGrid(options: {
  assetCount: number;
  tileSize: number;
  hasMore: boolean;
  isLoading: boolean;
  gallery: () => HTMLElement | undefined;
  grid: () => HTMLDivElement | undefined;
  scroller: () => HTMLElement | undefined;
  onReachEnd: () => void;
  onRange?: (range: LibraryRange) => void;
  rangeResetKey: number;
}) {
  const [geometry, setGeometry] = createSignal({ width: 0, margin: 0 });
  const [version, setVersion] = createSignal(0);
  const columns = createMemo(() =>
    Math.max(1, Math.floor((geometry().width + GAP) / (options.tileSize + GAP)))
  );
  const size = createMemo(() =>
    Math.max(
      1,
      Math.floor((Math.max(geometry().width, options.tileSize) - GAP * (columns() - 1)) / columns())
    )
  );
  const virtualizer = new Virtualizer<HTMLElement, Element>({
    count: 0,
    getScrollElement: () => options.scroller() ?? null,
    estimateSize: () => 188,
    scrollToFn: elementScroll,
    observeElementRect,
    observeElementOffset,
    onChange: () => setVersion((value) => value + 1)
  });
  const dispose = virtualizer._didMount();
  onCleanup(dispose);
  createEffect(
    () => ({ gallery: options.gallery(), grid: options.grid(), scroller: options.scroller() }),
    ({ gallery, grid, scroller }) => {
      if (!gallery || !grid || !scroller) return;
      const measure = () => {
        const width = grid.clientWidth;
        const margin =
          grid.getBoundingClientRect().top -
          scroller.getBoundingClientRect().top -
          scroller.clientTop +
          scroller.scrollTop;
        setGeometry((previous) =>
          previous.width === width && previous.margin === margin ? previous : { width, margin }
        );
      };
      measure();
      const observer = new ResizeObserver(measure);
      observer.observe(gallery);
      observer.observe(grid);
      observer.observe(scroller);
      window.addEventListener("resize", measure);
      return () => {
        observer.disconnect();
        window.removeEventListener("resize", measure);
      };
    }
  );
  let previous = { columns: 1, size: 188, margin: 0 };
  createEffect(
    () => ({
      count: geometry().width > 0 ? Math.ceil(options.assetCount / columns()) : 0,
      columns: columns(),
      size: size(),
      margin: geometry().margin,
      scroller: options.scroller()
    }),
    (next) => {
      const scrollTop = next.scroller?.scrollTop ?? 0;
      const firstIndex =
        Math.floor(Math.max(0, scrollTop - previous.margin) / (previous.size + GAP)) *
        previous.columns;
      virtualizer.setOptions({
        ...virtualizer.options,
        count: next.count,
        getItemKey: rowKey,
        getScrollElement: () => next.scroller ?? null,
        estimateSize: () => next.size,
        overscan: 4,
        gap: GAP,
        scrollMargin: next.margin
      });
      virtualizer._willUpdate();
      if (previous.columns !== next.columns || previous.size !== next.size) {
        virtualizer.measure();
        if (scrollTop > previous.margin)
          virtualizer.scrollToOffset(
            next.margin + Math.floor(firstIndex / next.columns) * (next.size + GAP)
          );
      }
      previous = next;
      setVersion((value) => value + 1);
    }
  );
  const snapshot = createMemo(() => {
    version();
    const rows = virtualizer.getVirtualItems();
    const columnCount = columns();
    const count = options.assetCount;
    const items = rows.flatMap((row) =>
      Array.from({ length: Math.min(columnCount, count - row.index * columnCount) }, (_, lane) => ({
        index: row.index * columnCount + lane,
        lane,
        start: row.start - geometry().margin
      }))
    );
    return {
      items,
      total: virtualizer.getTotalSize(),
      range: {
        startIndex: items[0]?.index ?? 0,
        endIndex: items[items.length - 1]?.index ?? -1,
        visibleStartIndex: (virtualizer.range?.startIndex ?? 0) * columnCount,
        visibleEndIndex: Math.min(
          count - 1,
          ((virtualizer.range?.endIndex ?? -1) + 1) * columnCount - 1
        )
      }
    };
  });
  createEffect(
    () => ({ range: snapshot().range, reset: options.rangeResetKey }),
    ({ range }) => options.onRange?.(range)
  );
  createEffect(
    () => ({
      more: options.hasMore,
      loading: options.isLoading,
      count: options.assetCount,
      end: snapshot().range.endIndex,
      columns: columns()
    }),
    (value) => {
      if (
        value.more &&
        !value.loading &&
        value.count > 0 &&
        value.end >= value.count - value.columns * 2
      )
        options.onReachEnd();
    }
  );
  return {
    get gridOffset() {
      return geometry().margin;
    },
    tileGap: GAP,
    get tilePixelSize() {
      return size();
    },
    get columnCount() {
      return columns();
    },
    get virtualItems() {
      return snapshot().items;
    },
    get totalSize() {
      return snapshot().total;
    },
    // Exposed for tests that observe the actual core measurement cache.
    virtualizer
  };
}
