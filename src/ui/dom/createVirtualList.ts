import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";

export interface VirtualListItem {
  index: number;
  start: number;
  size: number;
}
interface Options {
  scroller: () => HTMLElement | undefined;
  count: () => number;
  /** Size used until an item is measured; changing it re-lays out unmeasured items. */
  estimateSize: () => number;
  overscan?: number;
  /** Viewport height before the scroller reports a nonzero size. */
  initialViewport?: number;
  /** Observation runs only while enabled, e.g. while a modal is open. */
  enabled?: () => boolean;
}

/**
 * Vertical windowing for dialog lists: fixed estimated sizes, optionally replaced by
 * measured element heights. Offsets are prefix sums, so lookups stay O(log n). Item
 * objects keep their identity while their position and size are unchanged.
 */
export function createVirtualList(options: Options) {
  const overscan = options.overscan ?? 2;
  const fallback = options.initialViewport ?? 0;
  const [viewport, setViewport] = createSignal(fallback);
  const [offset, setOffset] = createSignal(0);
  const [measured, setMeasured] = createSignal(0);
  const sizes = new Map<number, number>();
  const elements = new Map<number, HTMLElement>();
  const indices = new WeakMap<Element, number>();
  const layout = createMemo(() => {
    measured();
    const count = Math.max(0, options.count());
    const estimate = options.estimateSize();
    const next = new Float64Array(count + 1);
    for (let index = 0; index < count; index++)
      next[index + 1] = next[index] + (sizes.get(index) ?? estimate);
    return { count, starts: next };
  });
  // First index whose end lies after `position`.
  const find = (values: Float64Array, count: number, position: number) => {
    let low = 0;
    let high = count - 1;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (values[middle + 1] <= position) low = middle + 1;
      else high = middle;
    }
    return low;
  };
  let cache = new Map<number, VirtualListItem>();
  const items = createMemo(() => {
    const { count, starts: values } = layout();
    const next = new Map<number, VirtualListItem>();
    if (!count) {
      cache = next;
      return [] as VirtualListItem[];
    }
    const top = Math.max(0, offset());
    const first = Math.max(0, find(values, count, top) - overscan);
    const last = Math.min(count - 1, find(values, count, top + Math.max(1, viewport()) - 1) + overscan);
    const result: VirtualListItem[] = [];
    for (let index = first; index <= last; index++) {
      const start = values[index];
      const size = values[index + 1] - start;
      const previous = cache.get(index);
      const item =
        previous && previous.start === start && previous.size === size
          ? previous
          : { index, start, size };
      next.set(index, item);
      result.push(item);
    }
    cache = next;
    return result;
  });
  let observer: ResizeObserver | undefined;
  const record = (element: HTMLElement) => {
    const index = indices.get(element);
    if (index === undefined || elements.get(index) !== element) return;
    if (!element.isConnected) {
      observer?.unobserve(element);
      elements.delete(index);
      return;
    }
    const size = element.offsetHeight;
    const previous = sizes.get(index) ?? untrack(options.estimateSize);
    if (size === previous) return;
    sizes.set(index, size);
    // Growth above the viewport would push visible content down; keep it in place.
    const scroller = untrack(options.scroller);
    if (scroller && untrack(layout).starts[index] < scroller.scrollTop)
      scroller.scrollTop += size - previous;
    setMeasured((value) => value + 1);
  };
  createEffect(
    () => ({ element: options.scroller(), enabled: options.enabled?.() ?? true }),
    ({ element, enabled }) => {
      if (!element || !enabled) return;
      const resize = () => setViewport(element.clientHeight || fallback);
      const scroll = () => setOffset(element.scrollTop);
      resize();
      scroll();
      const viewportObserver = new ResizeObserver(resize);
      viewportObserver.observe(element);
      observer = new ResizeObserver((entries) => {
        for (const entry of entries) if (entry.target instanceof HTMLElement) record(entry.target);
      });
      for (const item of elements.values()) observer.observe(item);
      element.addEventListener("scroll", scroll, { passive: true });
      return () => {
        element.removeEventListener("scroll", scroll);
        viewportObserver.disconnect();
        observer?.disconnect();
        observer = undefined;
      };
    }
  );
  onCleanup(() => {
    observer?.disconnect();
    elements.clear();
  });
  return {
    items,
    totalSize: () => {
      const { count, starts: values } = layout();
      return values[count];
    },
    /** Registers an attached item element; later size changes are observed. */
    measure(element: HTMLElement, index: number) {
      const previous = elements.get(index);
      if (previous && previous !== element) observer?.unobserve(previous);
      elements.set(index, element);
      indices.set(element, index);
      observer?.observe(element);
      record(element);
    },
    scrollToIndex(index: number, align: "auto" | "start" = "auto") {
      const scroller = untrack(options.scroller);
      const { count, starts: values } = untrack(layout);
      if (!scroller || index < 0 || index >= count) return;
      const start = values[index];
      const end = values[index + 1];
      const top = scroller.scrollTop;
      const height = untrack(viewport);
      const target =
        align === "start" || start < top ? start : end > top + height ? end - height : top;
      if (target === top) return;
      scroller.scrollTop = target;
      setOffset(scroller.scrollTop);
    }
  };
}
