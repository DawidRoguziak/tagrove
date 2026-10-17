import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import { Virtualizer, observeElementOffset, observeElementRect } from "@tanstack/virtual-core";
import { scrollCoordinates } from "./scrollCoordinates";
import type { LibraryRange, LibraryViewport } from "../../library/createLibrary";

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
  onRange?: (viewport: LibraryViewport | null) => void;
  rangeResetKey: number;
}) {
  const [geometry, setGeometry] = createSignal({ width: 0, margin: 0, viewport: 0 });
  const [version, setVersion] = createSignal(0);
  const [paused, setPaused] = createSignal(false);
  let lastOffset = 0;
  let motion: { time: number; distance: number }[] = [];
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const observeMotion = (offset: number) => {
    const distance = Math.abs(offset - lastOffset);
    lastOffset = offset;
    if (!distance) return;
    const now = performance.now();
    motion = motion.filter((sample) => now - sample.time <= 100);
    motion.push({ time: now, distance });
    const viewport = untrack(geometry).viewport;
    const fast = viewport > 0 && (distance >= viewport / 2 ||
      motion.reduce((sum, sample) => sum + sample.distance, 0) >= viewport);
    if (fast || idleTimer !== undefined) {
      setPaused(true);
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => {
        idleTimer = undefined;
        motion = [];
        setPaused(false);
      }, 150);
    }
  };
  onCleanup(() => {
    clearTimeout(idleTimer);
    options.onRange?.(null);
  });
  const columns = createMemo(() =>
    Math.max(1, Math.floor((geometry().width + GAP) / (options.tileSize + GAP)))
  );
  const size = createMemo(() =>
    Math.max(
      1,
      Math.floor((Math.max(geometry().width, options.tileSize) - GAP * (columns() - 1)) / columns())
    )
  );
  let coordinates = scrollCoordinates(0, 0, 0);
  let position = { physical: 0, logical: 0 };
  let publishOffset: ((offset: number, scrolling: boolean) => void) | undefined;
  const readOffset = (physical: number) => {
    // Retain subpixel logical wheel steps when WebKit rounds scrollTop.
    if (physical !== position.physical)
      position = { physical, logical: coordinates.toLogical(physical) };
    return position.logical;
  };
  const scrollTo = (logical: number, userMotion = false) => {
    const scroller = untrack(options.scroller);
    if (!scroller) return;
    const physical = Math.max(0, coordinates.toPhysical(logical));
    scroller.scrollTop = physical;
    const actual = scroller.scrollTop;
    position = {
      physical: actual,
      logical:
        Math.abs(actual - physical) < 1 ? Math.max(0, logical) : coordinates.toLogical(actual)
    };
    if (userMotion) observeMotion(position.logical);
    else {
      lastOffset = position.logical;
      motion = [];
    }
    publishOffset?.(position.logical, false);
    setVersion((value) => value + 1);
  };
  const scrollBy = (delta: number) => {
    const scroller = untrack(options.scroller);
    if (scroller) scrollTo(readOffset(scroller.scrollTop) + delta, true);
  };
  const virtualizer = new Virtualizer<HTMLElement, Element>({
    count: 0,
    getScrollElement: () => options.scroller() ?? null,
    estimateSize: () => 188,
    scrollToFn: (offset, { adjustments = 0 }) => scrollTo(offset + adjustments),
    observeElementRect,
    observeElementOffset: (instance, callback) => {
      publishOffset = callback;
      const cleanup = observeElementOffset(instance, (physical, scrolling) => {
        // virtual-core's debounced scroll-end can carry the pre-resize offset.
        // Read the current DOM value so it cannot retire a newer anchor write.
        const logical = readOffset(instance.scrollElement?.scrollTop ?? physical);
        if (scrolling) observeMotion(logical);
        callback(logical, scrolling);
        setVersion((value) => value + 1);
      });
      return () => {
        publishOffset = undefined;
        cleanup?.();
      };
    },
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
        const viewport = scroller.clientHeight;
        setGeometry((previous) =>
          previous.width === width && previous.margin === margin && previous.viewport === viewport
            ? previous
            : { width, margin, viewport }
        );
      };
      measure();
      const observer = new ResizeObserver(measure);
      observer.observe(gallery);
      observer.observe(grid);
      observer.observe(scroller);
      window.addEventListener("resize", measure);
      const wheel = (event: WheelEvent) => {
        if (!coordinates.compressed || event.ctrlKey || event.defaultPrevented || !event.deltaY)
          return;
        // An independently scrollable bulk inspector owns its own wheel input.
        if (event.target instanceof Element && !gallery.contains(event.target)) return;
        event.preventDefault();
        const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? scroller.clientHeight : 1;
        scrollBy(event.deltaY * unit);
      };
      scroller.addEventListener("wheel", wheel, { passive: false });
      return () => {
        scroller.removeEventListener("wheel", wheel);
        observer.disconnect();
        window.removeEventListener("resize", measure);
      };
    }
  );
  let previous = { columns: 1, size: 188, margin: 0, count: 0, viewport: 0 };
  createEffect(
    () => ({
      count: geometry().width > 0 ? Math.ceil(options.assetCount / columns()) : 0,
      columns: columns(),
      size: size(),
      margin: geometry().margin,
      viewport: geometry().viewport,
      scroller: options.scroller(),
      grid: options.grid()
    }),
    (next) => {
      const scrollTop = readOffset(next.scroller?.scrollTop ?? 0);
      const firstIndex =
        Math.floor(Math.max(0, scrollTop - previous.margin) / (previous.size + GAP)) *
        previous.columns;
      const total = Math.max(0, next.count * (next.size + GAP) - GAP);
      coordinates = scrollCoordinates(total, next.viewport, next.margin);
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
      }
      // Commit the bounded spacer before writing the anchor; the old spacer can
      // otherwise clamp the new scrollTop during a compressed/uncompressed resize.
      const grid = next.grid;
      if (grid) grid.style.height = `${coordinates.height}px`;
      if (scrollTop > previous.margin) {
        const anchor = next.margin + Math.floor(firstIndex / next.columns) * (next.size + GAP);
        const layoutChanged = previous.columns !== next.columns || previous.size !== next.size;
        scrollTo(layoutChanged ? anchor : scrollTop);
      } else scrollTo(scrollTop);
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
        start: row.start - geometry().margin - (position.logical - position.physical)
      }))
    );
    return {
      items,
      total: coordinates.height,
      shift: position.logical - position.physical,
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
  const range = createMemo<LibraryRange>((previous) => {
    const next = snapshot().range;
    return previous &&
      previous.startIndex === next.startIndex &&
      previous.endIndex === next.endIndex &&
      previous.visibleStartIndex === next.visibleStartIndex &&
      previous.visibleEndIndex === next.visibleEndIndex
      ? previous
      : next;
  });
  createEffect(
    () => ({ ...range(), paused: paused(), reset: options.rangeResetKey }),
    ({ reset: _reset, ...viewport }) => options.onRange?.(viewport)
  );
  createEffect(
    () => ({
      paused: paused(),
      more: options.hasMore,
      loading: options.isLoading,
      count: options.assetCount,
      end: range().endIndex,
      columns: columns()
    }),
    (value) => {
      if (
        !value.paused &&
        value.more &&
        !value.loading &&
        value.count > 0 &&
        value.end >= value.count - value.columns * 2
      )
        options.onReachEnd();
    }
  );
  return {
    get paused() {
      return paused();
    },
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
    get scrollShift() {
      return snapshot().shift;
    },
    scrollBy,
    // Exposed for tests that observe the actual core measurement cache.
    virtualizer
  };
}
