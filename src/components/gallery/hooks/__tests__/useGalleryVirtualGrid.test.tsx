import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGalleryVirtualGrid } from "../useGalleryVirtualGrid";

const measurements = vi.hoisted(() => ({ calls: 0 }));
vi.mock("@tanstack/react-virtual", async () => {
  const actual =
    await vi.importActual<typeof import("@tanstack/react-virtual")>("@tanstack/react-virtual");
  const estimates = new WeakMap<(index: number) => number, (index: number) => number>();
  return {
    ...actual,
    useVirtualizer: (options: Parameters<typeof actual.useVirtualizer>[0]) => {
      let estimate = estimates.get(options.estimateSize);
      if (!estimate) {
        const original = options.estimateSize;
        estimate = (index: number) => {
          measurements.calls++;
          return original(index);
        };
        estimates.set(original, estimate);
      }
      return actual.useVirtualizer({ ...options, estimateSize: estimate });
    }
  };
});

const resizeCallbacks = new Set<(entries: ResizeObserverEntry[]) => void>();
class Observer {
  constructor(callback: (entries: ResizeObserverEntry[]) => void) {
    resizeCallbacks.add(callback);
    this.callback = callback;
  }
  callback: (entries: ResizeObserverEntry[]) => void;
  observe() {}
  unobserve() {}
  disconnect() {
    resizeCallbacks.delete(this.callback);
  }
}

function mountGrid(count = 10_000) {
  const scroller = document.createElement("div");
  const gallery = document.createElement("section");
  const grid = document.createElement("div");
  scroller.append(gallery);
  gallery.append(grid);
  document.body.append(scroller);
  let width = 980;
  Object.defineProperties(scroller, {
    offsetWidth: { get: () => width },
    offsetHeight: { value: 600 },
    clientHeight: { value: 600 },
    scrollHeight: { value: 2_000_000 }
  });
  Object.defineProperty(grid, "clientWidth", { get: () => width });
  vi.spyOn(scroller, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, 0, width, 600)
  );
  vi.spyOn(grid, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, 80 - scroller.scrollTop, width, 2_000_000)
  );
  scroller.scrollTo = vi.fn((options?: ScrollToOptions | number) => {
    if (typeof options === "object") scroller.scrollTop = options.top ?? 0;
  });
  const options = {
    assetCount: count,
    getAssetAt: vi.fn(() => undefined),
    tileSize: 188,
    hasMore: false,
    isLoading: false,
    galleryRef: { current: gallery },
    gridRef: { current: grid },
    scrollContainerRef: { current: scroller },
    onReachEnd: vi.fn(),
    onVirtualRangeChange: vi.fn()
  };
  const hook = renderHook((props) => useGalleryVirtualGrid(props), { initialProps: options });
  return {
    ...hook,
    options,
    scroller,
    setWidth: (value: number) => {
      width = value;
    }
  };
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", Observer);
  measurements.calls = 0;
});
afterEach(() => {
  cleanup();
  document.body.replaceChildren();
  vi.unstubAllGlobals();
});

describe("gallery row virtualization with the real TanStack virtualizer", () => {
  it("does not rebuild all measurements on scroll, page arrival, or status renders", () => {
    const { result, rerender, options, scroller } = mountGrid();
    expect(result.current.columnCount).toBe(5);
    expect(result.current.totalSize).toBe(2000 * 198 - 10);
    expect(result.current.virtualItems.length).toBeLessThan(70);
    measurements.calls = 0;
    act(() => {
      scroller.scrollTop = 20_000;
      scroller.dispatchEvent(new Event("scroll"));
    });
    rerender({ ...options, getAssetAt: vi.fn(() => undefined) });
    rerender({ ...options, isLoading: true });
    expect(measurements.calls).toBe(0);
    expect(result.current.virtualItems[0].index).toBeGreaterThan(450);
    expect(result.current.virtualItems.length).toBeLessThan(70);
    const range = options.onVirtualRangeChange.mock.lastCall?.[0];
    expect(range).toMatchObject({ visibleStartIndex: 500 });
  });

  it("keeps the first visible asset in view when columns change", () => {
    const { result, scroller, setWidth } = mountGrid();
    act(() => {
      scroller.scrollTop = 80 + 198 * 10;
      scroller.dispatchEvent(new Event("scroll"));
    });
    setWidth(584);
    act(() => {
      for (const callback of resizeCallbacks) callback([]);
    });
    act(() => {
      scroller.dispatchEvent(new Event("scroll"));
    });
    expect(result.current.columnCount).toBe(3);
    expect(scroller.scrollTop).toBe(80 + 16 * 198);
    expect(result.current.virtualItems.some((item) => item.index === 50)).toBe(true);
  });

  it("clamps the final row and accounts for the grid offset in the scroll container", () => {
    const { result, options } = mountGrid(7);
    expect(result.current.virtualItems.map((item) => item.index)).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(result.current.virtualItems[0].start).toBe(0);
    expect(result.current.virtualItems[5]).toMatchObject({ lane: 0, start: 198 });
    expect(result.current.totalSize).toBe(386);
    expect(options.onVirtualRangeChange).toHaveBeenLastCalledWith({
      startIndex: 0,
      endIndex: 6,
      visibleStartIndex: 0,
      visibleEndIndex: 6
    });
  });
});
