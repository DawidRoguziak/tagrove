import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import type { LibraryRange, LibraryViewport } from "../../library/createLibrary";
import {
  anchorForResize,
  centeredBase,
  clampBase,
  EMPTY_ROWS,
  GRID_GAP,
  gridLayout,
  type GridLayout,
  indexToRow,
  maxBase,
  MAX_GRID_HEIGHT,
  needsRebase,
  OVERSCAN_ROWS,
  poolRows,
  rowRange,
  type RowRange,
  rowTop,
  toLibraryRange,
  windowHeight
} from "./gridGeometry";

const IDLE_MS = 150;
const MOTION_WINDOW_MS = 100;
const sameRows = (a: RowRange, b: RowRange) =>
  a.first === b.first &&
  a.last === b.last &&
  a.visibleFirst === b.visibleFirst &&
  a.visibleLast === b.visibleLast;
const editable = 'input, textarea, select, [contenteditable="true"]';

/**
 * Square-tile grid over a native scroll container. Offsets are logical pixels relative
 * to the grid top. Up to MAX_GRID_HEIGHT the spacer is the content (`base` stays 0).
 * Taller content scrolls natively inside a physical window that starts at logical
 * `base`; the window moves only while idle near its edges, or immediately when native
 * scrolling would otherwise run out of window.
 */
export function createVirtualGrid(options: {
  assetCount: number;
  tileSize: number;
  gallery: () => HTMLElement | undefined;
  grid: () => HTMLDivElement | undefined;
  scroller: () => HTMLElement | undefined;
  onRange?: (viewport: LibraryViewport | null) => void;
  rangeResetKey: number;
}) {
  const [geometry, setGeometry] = createSignal({ width: 0, margin: 0, viewport: 0 });
  const [paused, setPaused] = createSignal(false);
  const [base, setBase] = createSignal(0);
  const [offset, setOffset] = createSignal(0);
  const layout = createMemo(() =>
    gridLayout({
      width: geometry().width,
      tileMin: options.tileSize,
      gap: GRID_GAP,
      count: geometry().width > 0 ? options.assetCount : 0
    })
  );
  const windowed = createMemo(() => layout().contentHeight > MAX_GRID_HEIGHT);
  // Rows are published together with the layout they were computed for, so a range
  // is never derived from a new column count and a not-yet-anchored offset.
  const [view, setView] = createSignal<{ layout: GridLayout; rows: RowRange }>({
    layout: untrack(layout),
    rows: EMPTY_ROWS
  });

  let lastOffset = 0;
  let motion: { time: number; distance: number }[] = [];
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let rebaseTimer: ReturnType<typeof setTimeout> | undefined;
  // Retain subpixel logical steps (edge scrolling) while WebKit rounds scrollTop.
  let position = { physical: Number.NaN, logical: 0 };
  const observeMotion = (logical: number) => {
    const distance = Math.abs(logical - lastOffset);
    lastOffset = logical;
    if (!distance) return;
    const now = performance.now();
    motion = motion.filter((sample) => now - sample.time <= MOTION_WINDOW_MS);
    motion.push({ time: now, distance });
    const viewport = untrack(geometry).viewport;
    const fast =
      viewport > 0 &&
      (distance >= viewport / 2 ||
        motion.reduce((sum, sample) => sum + sample.distance, 0) >= viewport);
    if (fast || idleTimer !== undefined) {
      setPaused(true);
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => {
        idleTimer = undefined;
        motion = [];
        setPaused(false);
      }, IDLE_MS);
    }
  };
  /**
   * Explicit demand pause for a timeline drag: pages and thumbnails wait until
   * `resumeDemand` (pointer release) or IDLE_MS without another hold (pointer at rest).
   */
  const holdDemand = () => {
    setPaused(true);
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => {
      idleTimer = undefined;
      motion = [];
      setPaused(false);
    }, IDLE_MS);
  };
  const resumeDemand = () => {
    if (idleTimer === undefined && !untrack(paused)) return;
    clearTimeout(idleTimer);
    idleTimer = undefined;
    motion = [];
    setPaused(false);
  };
  const publish = (logical: number) => {
    const current = untrack(layout);
    const rows = rowRange(current, logical, untrack(geometry).viewport, OVERSCAN_ROWS);
    setView((previous) =>
      previous.layout === current && sameRows(previous.rows, rows) ? previous : { layout: current, rows }
    );
    if (untrack(windowed)) setOffset(logical);
  };
  const readOffset = (physical: number) => {
    if (physical !== position.physical)
      position = { physical, logical: untrack(base) + physical - untrack(geometry).margin };
    return position.logical;
  };
  /**
   * Writes a logical offset. The window moves first when the target would come within two
   * viewports of an edge it could still move past, or always with `recenter`.
   */
  const scrollTo = (target: number, userMotion = false, recenter = false) => {
    const scroller = untrack(options.scroller);
    if (!scroller) return;
    const { margin, viewport } = untrack(geometry);
    const content = untrack(layout).contentHeight;
    const logical = Math.max(-margin, Math.min(target, content));
    let nextBase = clampBase(untrack(base), content);
    if (recenter || needsRebase(logical, viewport, nextBase, content, 2 * viewport))
      nextBase = centeredBase(logical, viewport, content);
    if (nextBase !== untrack(base)) setBase(nextBase);
    const physical = margin + logical - nextBase;
    scroller.scrollTop = physical;
    const actual = scroller.scrollTop;
    position = {
      physical: actual,
      logical: Math.abs(actual - physical) < 1 ? logical : nextBase + actual - margin
    };
    if (userMotion) observeMotion(position.logical);
    else {
      lastOffset = position.logical;
      motion = [];
    }
    publish(position.logical);
  };
  const rebaseWhenIdle = () => {
    clearTimeout(rebaseTimer);
    rebaseTimer = setTimeout(() => {
      rebaseTimer = undefined;
      const { viewport } = untrack(geometry);
      const content = untrack(layout).contentHeight;
      if (needsRebase(position.logical, viewport, untrack(base), content, windowHeight(content) / 4))
        scrollTo(position.logical, false, true);
    }, IDLE_MS);
  };
  const handleScroll = () => {
    const scroller = untrack(options.scroller);
    if (!scroller) return;
    const logical = readOffset(scroller.scrollTop);
    observeMotion(logical);
    if (!untrack(windowed)) {
      publish(logical);
      return;
    }
    const { viewport } = untrack(geometry);
    // Native scrolling must never reach a window edge the base could still move past.
    if (needsRebase(logical, viewport, untrack(base), untrack(layout).contentHeight, 2 * viewport))
      scrollTo(logical);
    else publish(logical);
    rebaseWhenIdle();
  };
  const scrollBy = (delta: number) => {
    const scroller = untrack(options.scroller);
    // Rectangle selection calls this every frame, mostly with zero outside its edge zones.
    if (scroller && delta) scrollTo(readOffset(scroller.scrollTop) + delta, true);
  };
  onCleanup(() => {
    clearTimeout(idleTimer);
    clearTimeout(rebaseTimer);
    options.onRange?.(null);
  });

  createEffect(
    () => ({ gallery: options.gallery(), grid: options.grid(), scroller: options.scroller() }),
    ({ gallery, grid, scroller }) => {
      if (!gallery || !grid || !scroller) return;
      // Geometry comes only from observers; the scroll path reads nothing but scrollTop.
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
      // Native Home/End would only reach the ends of the physical window. Recycling a
      // focused tile's row can drop focus to the body, so listen on the window too.
      const keydown = (event: KeyboardEvent) => {
        if (!untrack(windowed) || event.defaultPrevented || event.altKey || event.metaKey) return;
        if (event.key !== "Home" && event.key !== "End") return;
        const target = event.target;
        const owned =
          target === document.body ||
          target === document.documentElement ||
          target === scroller ||
          (target instanceof Node && gallery.contains(target));
        // The bulk dock, overlays and text controls keep their own Home/End.
        if (!owned || (target instanceof Element && target.closest(editable))) return;
        event.preventDefault();
        scrollTo(event.key === "Home" ? Number.NEGATIVE_INFINITY : Number.POSITIVE_INFINITY);
      };
      scroller.addEventListener("scroll", handleScroll, { passive: true });
      window.addEventListener("keydown", keydown);
      return () => {
        scroller.removeEventListener("scroll", handleScroll);
        window.removeEventListener("keydown", keydown);
        observer.disconnect();
        window.removeEventListener("resize", measure);
      };
    }
  );

  // Without its native bar the windowed scroller pads the gutter back, keeping columns.
  createEffect(
    () => ({ scroller: options.scroller(), windowed: windowed() }),
    ({ scroller, windowed }) => {
      if (!scroller || !windowed) return;
      const gutter = Math.max(0, scroller.offsetWidth - scroller.clientWidth - 2 * scroller.clientLeft);
      scroller.dataset.galleryWindowed = "";
      scroller.style.paddingRight = `${gutter}px`;
      return () => {
        delete scroller.dataset.galleryWindowed;
        scroller.style.paddingRight = "";
      };
    }
  );

  let previous: { layout: GridLayout; margin: number; scroller: HTMLElement | undefined } | undefined;
  createEffect(
    () => ({
      layout: layout(),
      margin: geometry().margin,
      viewport: geometry().viewport,
      scroller: options.scroller(),
      grid: options.grid()
    }),
    (next) => {
      const scroller = next.scroller;
      const content = next.layout.contentHeight;
      // Commit the spacer before any anchor write so the old height cannot clamp it.
      if (next.grid) next.grid.style.height = `${windowHeight(content)}px`;
      const same = previous?.scroller === scroller ? previous : undefined;
      previous = { layout: next.layout, margin: next.margin, scroller };
      if (!scroller) {
        publish(0);
        return;
      }
      const physical = scroller.scrollTop;
      const currentBase = untrack(base);
      const exact = same && position.physical === physical;
      const prior = same?.layout ?? next.layout;
      if (prior.columns !== next.layout.columns || prior.size !== next.layout.size) {
        const before = exact ? position.logical : currentBase + physical - (same?.margin ?? next.margin);
        // Above the grid top (the first measurement included) the position stays physical.
        if (before > 0) {
          scrollTo(anchorForResize(prior, next.layout, before));
          return;
        }
      }
      // Margin-only changes (an error banner) move the grid; the scroll position stays.
      const logical =
        exact && same.margin === next.margin ? position.logical : currentBase + physical - next.margin;
      if (
        clampBase(currentBase, content) !== currentBase ||
        needsRebase(logical, next.viewport, currentBase, content, 2 * next.viewport)
      )
        scrollTo(logical);
      else {
        position = { physical, logical };
        lastOffset = logical;
        publish(logical);
      }
    }
  );

  const range = createMemo<LibraryRange>((previous) => {
    const next = toLibraryRange(view().layout, view().rows);
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
  // Consumers such as the selection handlers key effects on these values; memos keep them
  // from rerunning (and cancelling a drag) on every published row change.
  const margin = createMemo(() => geometry().margin);
  const columns = createMemo(() => view().layout.columns);
  const size = createMemo(() => view().layout.size);
  const rowCount = createMemo(() => view().layout.rows);
  const firstRow = createMemo(() => view().rows.first);
  const pool = createMemo(() => poolRows(view().layout, geometry().viewport, OVERSCAN_ROWS));
  const physicalHeight = createMemo(() => windowHeight(view().layout.contentHeight));
  const maxOffset = () => {
    const { margin, viewport } = geometry();
    return Math.max(-margin, layout().contentHeight - viewport);
  };
  return {
    get paused() {
      return paused();
    },
    get gridOffset() {
      return margin();
    },
    tileGap: GRID_GAP,
    get tilePixelSize() {
      return size();
    },
    get columnCount() {
      return columns();
    },
    get rowCount() {
      return rowCount();
    },
    /** First rendered row (overscan included); the row pool is keyed on it. */
    get firstRow() {
      return firstRow();
    },
    get range() {
      return range();
    },
    get poolSize() {
      return pool();
    },
    get viewport() {
      return geometry().viewport;
    },
    /** Logical px of the physical grid top; rows render at `rowTop - scrollShift`. */
    get scrollShift() {
      return base();
    },
    get physicalHeight() {
      return physicalHeight();
    },
    get windowed() {
      return windowed();
    },
    /** Logical viewport top, tracked per frame only while windowed (custom scrollbar). */
    get offset() {
      return offset();
    },
    get minOffset() {
      return -geometry().margin;
    },
    get maxOffset() {
      return maxOffset();
    },
    scrollBy,
    /** Logical offset write; `userMotion` lets a custom-bar drag pause page demand. */
    scrollTo: (logical: number, userMotion = false) => scrollTo(logical, userMotion),
    scrollToIndex: (index: number) => scrollTo(rowTop(untrack(layout), indexToRow(untrack(layout), index))),
    /** Logical offset of the row holding `index` (O(1); fixed row height). */
    offsetOfIndex: (index: number) => rowTop(untrack(layout), indexToRow(untrack(layout), index)),
    holdDemand,
    resumeDemand,
    get maxBase() {
      return maxBase(layout().contentHeight);
    }
  };
}
