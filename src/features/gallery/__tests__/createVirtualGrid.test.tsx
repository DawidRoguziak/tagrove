import { cleanup, renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createVirtualGrid } from "../createVirtualGrid";
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
});
function mount(count = 10000, delayed = false) {
  const scroller = document.createElement("div"),
    gallery = document.createElement("section"),
    grid = document.createElement("div");
  scroller.append(gallery);
  gallery.append(grid);
  document.body.append(scroller);
  let width = 988;
  Object.defineProperties(scroller, {
    offsetWidth: { get: () => width },
    offsetHeight: { value: 600 },
    clientHeight: { value: 600 },
    scrollHeight: { value: 2000000 }
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
  const view = renderHook(() => {
    const [ready, setReady] = createSignal(!delayed);
    const [loading, setLoading] = createSignal(false);
    const [total, setTotal] = createSignal(count);
    const [reset, setReset] = createSignal(0);
    const model = createVirtualGrid({
      get assetCount() {
        return total();
      },
      tileSize: 188,
      hasMore: false,
      get isLoading() {
        return loading();
      },
      gallery: () => (ready() ? gallery : undefined),
      grid: () => (ready() ? grid : undefined),
      scroller: () => (ready() ? scroller : undefined),
      onReachEnd: vi.fn(),
      onRange: range,
      get rangeResetKey() {
        return reset();
      }
    });
    return { model, setReady, setLoading, setTotal, setReset };
  });
  flush();
  return {
    ...view,
    scroller,
    range,
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
