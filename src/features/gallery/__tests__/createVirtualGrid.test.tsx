import { cleanup, renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createVirtualGrid } from "../createVirtualGrid";
import { MAX_GRID_HEIGHT } from "../scrollCoordinates";
const callbacks = new Set<(entries: ResizeObserverEntry[]) => void>();
class Observer {
  constructor(readonly callback: (entries: ResizeObserverEntry[]) => void) {}
  observe() {
    callbacks.add(this.callback);
  }
  unobserve() {
    callbacks.delete(this.callback);
  }
  disconnect() {
    callbacks.delete(this.callback);
  }
}
beforeEach(() => vi.stubGlobal("ResizeObserver", Observer));
afterEach(() => {
  cleanup();
  document.body.replaceChildren();
  callbacks.clear();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
function mount(count = 10000, delayed = false, hasMore = false) {
  const scroller = document.createElement("div"),
    gallery = document.createElement("section"),
    grid = document.createElement("div");
  scroller.append(gallery);
  gallery.append(grid);
  document.body.append(scroller);
  let width = 988;
  let top = 0;
  let height = 600;
  Object.defineProperties(scroller, {
    offsetWidth: { get: () => width },
    offsetHeight: { get: () => height },
    clientHeight: { get: () => height },
    scrollHeight: { get: () => Number.parseFloat(grid.style.height || "0") + 80 + 136 },
    scrollTop: {
      get: () => top,
      set: (value: number) => {
        top = Math.round(Math.max(0, Math.min(value, scroller.scrollHeight - height)));
      }
    }
  });
  Object.defineProperty(grid, "clientWidth", { get: () => width });
  vi.spyOn(scroller, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, 0, width, 600)
  );
  vi.spyOn(grid, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, 80 - scroller.scrollTop, width, 2000000)
  );
  scroller.scrollTo = vi.fn((arg?: ScrollToOptions | number) => {
    if (typeof arg === "object") scroller.scrollTop = arg.top ?? 0;
  });
  const range = vi.fn();
  const reachEnd = vi.fn();
  const view = renderHook(() => {
    const [ready, setReady] = createSignal(!delayed);
    const [tile, setTile] = createSignal(188);
    const [loading, setLoading] = createSignal(false);
    const [total, setTotal] = createSignal(count);
    const [reset, setReset] = createSignal(0);
    const model = createVirtualGrid({
      get assetCount() {
        return total();
      },
      get tileSize() {
        return tile();
      },
      hasMore,
      get isLoading() {
        return loading();
      },
      gallery: () => (ready() ? gallery : undefined),
      grid: () => (ready() ? grid : undefined),
      scroller: () => (ready() ? scroller : undefined),
      onReachEnd: reachEnd,
      onRange: range,
      get rangeResetKey() {
        return reset();
      }
    });
    return { model, setReady, setLoading, setTotal, setReset, setTile };
  });
  flush();
  return {
    ...view,
    scroller,
    gallery,
    range,
    reachEnd,
    resizeHeight(next: number) {
      height = next;
      for (const callback of callbacks) callback([]);
      flush();
    },
    resize(next: number) {
      width = next;
      for (const callback of callbacks) callback([]);
      flush();
    },
    scroll(top: number) {
      scroller.scrollTop = top;
      scroller.dispatchEvent(new Event("scroll"));
      flush();
    }
  };
}
describe("Solid adapter over real virtual-core", () => {
  it.each([
    10000, 26000
  ])("bounds every initial and remount range with %i slots and null parent refs", (count) => {
    for (let pass = 0; pass < 2; pass++) {
      const view = mount(count, true),
        model = view.result.model;
      expect(model.virtualItems).toHaveLength(0);
      view.result.setReady(true);
      flush();
      expect(model.columnCount).toBe(5);
      expect(model.virtualItems.length).toBeGreaterThan(0);
      expect(model.virtualItems.length).toBeLessThan(70);
      for (const [range] of view.range.mock.calls)
        expect(range.endIndex - range.startIndex + 1).toBeLessThan(70);
      view.scroll(20080);
      expect(model.virtualItems[0].index).toBeGreaterThan(450);
      expect(model.virtualItems.length).toBeLessThan(70);
      view.cleanup();
      expect(callbacks.size).toBe(0);
    }
  });
  it("retains measurements through scrolling, progress, and range retries", () => {
    const view = mount(),
      model = view.result.model;
    const cache = model.virtualizer.measurementsCache;
    expect(model.totalSize).toBe(2000 * 200 - 12);
    view.scroll(20080);
    view.result.setLoading(true);
    flush();
    expect(model.virtualizer.measurementsCache).toBe(cache);
    view.range.mockClear();
    view.result.setReset(1);
    flush();
    expect(view.range).toHaveBeenCalledOnce();
    expect(view.range).toHaveBeenLastCalledWith(
      expect.objectContaining({ visibleStartIndex: 500 })
    );
    expect(model.virtualizer.measurementsCache).toBe(cache);
  });
  it("anchors resizing on the first visible global index and respects the external margin", () => {
    const view = mount();
    view.scroll(2080);
    view.resize(588);
    view.scroll(view.scroller.scrollTop);
    expect(view.result.model.columnCount).toBe(3);
    expect(view.scroller.scrollTop).toBe(3280);
    expect(view.result.model.virtualItems.some((item) => item.index === 50)).toBe(true);
  });
  it("reaches the millionth slot with bounded physical height, demand and mounted rows", () => {
    const view = mount(1_000_000);
    const model = view.result.model;
    expect(model.totalSize).toBe(MAX_GRID_HEIGHT);
    view.scroll(Number.MAX_SAFE_INTEGER);
    expect(model.virtualItems[model.virtualItems.length - 1]?.index).toBe(999_999);
    expect(model.virtualItems.length).toBeLessThan(70);
    expect(view.range).toHaveBeenLastCalledWith(
      expect.objectContaining({ endIndex: 999_999, visibleEndIndex: 999_999 })
    );
    const last = model.virtualItems[model.virtualItems.length - 1];
    expect(last && last.start + 80 - view.scroller.scrollTop).toBeGreaterThan(0);
    expect(last && last.start + 188 + 80 - view.scroller.scrollTop).toBeLessThanOrEqual(600);
    // A compressed one-column layout used to saturate even earlier.
    view.resize(188);
    view.scroll(Number.MAX_SAFE_INTEGER);
    expect(model.columnCount).toBe(1);
    expect(model.virtualItems[model.virtualItems.length - 1]?.index).toBe(999_999);
    expect(model.totalSize).toBe(MAX_GRID_HEIGHT);
    view.cleanup();
    expect(callbacks.size).toBe(0);
  });

  it("preserves global anchors across compressed resize, tile size, and uncompressed transitions", () => {
    const view = mount(1_000_000);
    view.scroll(MAX_GRID_HEIGHT / 2);
    const first = view.range.mock.lastCall?.[0].visibleStartIndex;
    view.resize(588);
    expect(view.range.mock.lastCall?.[0].visibleStartIndex).toBe(Math.floor(first / 3) * 3);
    view.result.setTile(340);
    flush();
    expect(view.range.mock.lastCall?.[0].visibleStartIndex).toBe(Math.floor(first / 3) * 3);
    const beforeHeight = view.range.mock.lastCall?.[0].visibleStartIndex;
    view.resizeHeight(450);
    expect(view.range.mock.lastCall?.[0].visibleStartIndex).toBe(beforeHeight);
    view.result.setTotal(100);
    flush();
    expect(view.result.model.totalSize).toBeLessThan(MAX_GRID_HEIGHT);
    expect(view.result.model.virtualItems[view.result.model.virtualItems.length - 1]?.index).toBe(
      99
    );
  });

  it("keeps small wheel and edge-scroll deltas logical despite rounded physical offsets", () => {
    const view = mount(1_000_000);
    view.resize(188);
    view.scroll(2_000_000);
    const before = view.result.model.virtualizer.scrollOffset ?? 0;
    view.range.mockClear();
    for (let i = 0; i < 10; i++) {
      view.gallery.dispatchEvent(
        new WheelEvent("wheel", { deltaY: 1, bubbles: true, cancelable: true })
      );
      view.scroller.dispatchEvent(new Event("scroll"));
      flush();
    }
    expect(view.result.model.virtualizer.scrollOffset).toBeCloseTo(before + 10);
    expect(view.range).not.toHaveBeenCalled();
    view.result.model.scrollBy(25);
    flush();
    expect(view.result.model.virtualizer.scrollOffset).toBeCloseTo(before + 35);
    const zoom = new WheelEvent("wheel", {
      deltaY: 100,
      ctrlKey: true,
      bubbles: true,
      cancelable: true
    });
    view.gallery.dispatchEvent(zoom);
    expect(zoom.defaultPrevented).toBe(false);
    const offset = view.scroller.scrollTop;
    view.cleanup();
    view.gallery.dispatchEvent(
      new WheelEvent("wheel", { deltaY: 200, bubbles: true, cancelable: true })
    );
    expect(view.scroller.scrollTop).toBe(offset);
  });

  it("ignores a delayed scroll-end offset captured before a resize anchor write", () => {
    vi.useFakeTimers();
    try {
      const view = mount(1_000_000);
      view.resize(588);
      view.result.setTile(340);
      flush();
      view.scroll(2_000_000);
      const before = view.result.model.virtualizer.scrollOffset;
      view.resizeHeight(450);
      expect(view.result.model.virtualizer.scrollOffset).toBe(before);
      vi.advanceTimersByTime(200);
      flush();
      expect(view.result.model.virtualizer.scrollOffset).toBe(before);
      view.cleanup();
    } finally {
      vi.useRealTimers();
    }
  });

  it("retains a partial last row and no slots when empty", () => {
    const view = mount(12);
    expect(view.result.model.virtualItems.map((item) => item.index)).toEqual(
      Array.from({ length: 12 }, (_, i) => i)
    );
    expect(view.result.model.totalSize).toBe(588);
    view.result.setTotal(0);
    flush();
    expect(view.result.model.virtualItems).toEqual([]);
  });
});

it("pauses on a half-viewport jump and resumes the same range after 150ms without movement", () => {
  vi.useFakeTimers();
  const view = mount();
  view.scroll(300);
  expect(view.result.model.paused).toBe(true);
  const latest = view.range.mock.lastCall?.[0];
  vi.advanceTimersByTime(149); flush();
  expect(view.result.model.paused).toBe(true);
  // Duplicate scroll/end notifications must not extend the pause.
  view.scroll(300);
  vi.advanceTimersByTime(1); flush();
  expect(view.range).toHaveBeenLastCalledWith({ ...latest, paused: false });
  view.cleanup();
  vi.useRealTimers();
});

it("keeps slow scrolling active and counts distance across direction changes in a rolling 100ms", () => {
  vi.useFakeTimers();
  const view = mount();
  for (let top = 100; top <= 500; top += 100) {
    view.scroll(top);
    vi.advanceTimersByTime(101); flush();
    expect(view.result.model.paused).toBe(false);
  }
  view.scroll(700);
  vi.advanceTimersByTime(30);
  view.scroll(500);
  vi.advanceTimersByTime(30);
  view.scroll(700);
  expect(view.result.model.paused).toBe(true);
  vi.advanceTimersByTime(100);
  view.scroll(690);
  vi.advanceTimersByTime(149); flush();
  expect(view.result.model.paused).toBe(true);
  vi.advanceTimersByTime(1); flush();
  expect(view.result.model.paused).toBe(false);
  view.cleanup();
  vi.useRealTimers();
});

it("excludes geometry anchors from motion and clears paused demand and timers on disposal", () => {
  vi.useFakeTimers();
  const view = mount(1_000_000);
  view.scroll(2_000_000);
  vi.advanceTimersByTime(150); flush();
  view.resize(588);
  view.result.setTile(340); flush();
  view.resizeHeight(450);
  view.scroll(view.scroller.scrollTop);
  expect(view.result.model.paused).toBe(false);
  view.result.model.scrollBy(225); flush();
  expect(view.result.model.paused).toBe(true);
  view.cleanup();
  expect(view.range).toHaveBeenLastCalledWith(null);
  view.range.mockClear();
  vi.advanceTimersByTime(1000); flush();
  expect(view.range).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
  vi.useRealTimers();
});


it("does not reach-end load during a paused jump, including after loading finishes", () => {
  vi.useFakeTimers();
  const view = mount(10000, false, true);
  view.scroll(Number.MAX_SAFE_INTEGER);
  expect(view.result.model.paused).toBe(true);
  view.result.setLoading(true); flush();
  view.result.setLoading(false); flush();
  expect(view.reachEnd).not.toHaveBeenCalled();
  vi.advanceTimersByTime(150); flush();
  expect(view.reachEnd).toHaveBeenCalledOnce();
  view.cleanup();
});
