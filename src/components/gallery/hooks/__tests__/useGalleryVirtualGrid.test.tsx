import { act, cleanup, render, renderHook } from "@testing-library/react";
import { StrictMode, useLayoutEffect, useRef } from "react";
import type { RefObject } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGalleryVirtualGrid } from "../useGalleryVirtualGrid";
import type { GalleryRange } from "../useGalleryVirtualGrid";
import { useLibraryAssets } from "../../../../hooks/useLibraryAssets";

const apiMocks = vi.hoisted(() => ({ startAssetQuery: vi.fn(), getAssetQueryPage: vi.fn() }));
vi.mock("../../../../api", () => apiMocks);

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
    this.callback = callback;
  }
  callback: (entries: ResizeObserverEntry[]) => void;
  targets = new Set<Element>();
  observe(target: Element) {
    this.targets.add(target);
    resizeCallbacks.add(this.callback);
  }
  unobserve(target: Element) {
    this.targets.delete(target);
    if (this.targets.size === 0) resizeCallbacks.delete(this.callback);
  }
  disconnect() {
    this.targets.clear();
    resizeCallbacks.delete(this.callback);
  }
}

function mountGrid(count = 10_000, externalScroller = true) {
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
    galleryRef: { current: externalScroller ? gallery : scroller },
    gridRef: { current: grid },
    scrollContainerRef: externalScroller ? { current: scroller } : undefined,
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
  vi.restoreAllMocks();
});

describe("gallery row virtualization with the real TanStack virtualizer", () => {
  it("uses the gallery as the scroller when no external ref is supplied", () => {
    const { result, scroller } = mountGrid(26_000, false);
    expect(result.current.columnCount).toBe(5);
    expect(result.current.virtualItems.length).toBeGreaterThan(0);
    expect(result.current.virtualItems.length).toBeLessThan(70);
    act(() => {
      scroller.scrollTop = 20_000;
      scroller.dispatchEvent(new Event("scroll"));
    });
    expect(result.current.virtualItems[0].index).toBeGreaterThan(450);
    expect(result.current.virtualItems.length).toBeLessThan(70);
  });

  it.each([false, true])("bounds every mount and Settings-return commit and page demand (Strict Mode: %s)", async (strict) => {
    const count = 26_000;
    let width = 980;
    // jsdom has no layout. The gallery grows with its spacer, while its parent
    // remains a 600px viewport. These getters run before React attaches refs.
    const contentHeight = (element: HTMLElement) =>
      Number.parseFloat(element.querySelector<HTMLElement>("[data-spacer]")?.style.height ?? "0");
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.dataset.scroller !== undefined ? 600 : contentHeight(this);
    });
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.dataset.scroller !== undefined ? 600 : contentHeight(this);
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(() => width);
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => width);
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
      return contentHeight(this);
    });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const scroller = this.closest<HTMLElement>("[data-scroller]");
      return new DOMRect(0, this === scroller ? 0 : 80 - (scroller?.scrollTop ?? 0), width, this.offsetHeight);
    });
    const commits: number[] = [];
    const ranges: GalleryRange[] = [];
    apiMocks.startAssetQuery.mockResolvedValue({
      status: "ready", session_id: 1, revision: 1, total: count, offset: 0,
      items: Array.from({ length: 128 }, (_, index) => ({
        id: index + 1, file_name: `photo-${index}.jpg`, preview_path: null,
        kind: "image", modified_at: 0, width: 10, height: 10, duration_ms: null,
        thumb_path: null, is_favorite: false, media_group_key: null, media_group_order: null
      }))
    });
    // Keep pages pending so the test observes demand without page arrivals
    // changing the lifecycle under test.
    apiMocks.getAssetQueryPage.mockReset().mockImplementation(() => new Promise(() => {}));
    const library = renderHook(() => useLibraryAssets({
      pageSize: 128,
      filterInclude: [],
      filterExclude: [],
      appliedMediaKind: "all",
      appliedFavoritesOnly: false,
      setThumbs: vi.fn(),
      resetThumbnailQueue: vi.fn()
    }));
    await act(async () => { await library.result.current.refresh(); });
    const onRange = (range: GalleryRange) => {
      ranges.push(range);
      library.result.current.ensureRange(range.startIndex, range.endIndex);
    };
    const onReachEnd = vi.fn();
    const getAssetAt = () => undefined;
    function Grid({ scrollContainerRef }: { scrollContainerRef: RefObject<HTMLElement | null> }) {
      const galleryRef = useRef<HTMLElement>(null);
      const gridRef = useRef<HTMLDivElement>(null);
      const grid = useGalleryVirtualGrid({
        assetCount: count, getAssetAt, tileSize: 188, hasMore: true, isLoading: false,
        galleryRef, gridRef, scrollContainerRef, onReachEnd, onVirtualRangeChange: onRange
      });
      useLayoutEffect(() => { commits.push(grid.virtualItems.length); });
      return <section ref={galleryRef}>
        <div ref={gridRef} data-spacer style={{ height: grid.totalSize }}>
          {grid.virtualItems.map(item => <div key={item.index} data-slot={item.index} />)}
        </div>
      </section>;
    }
    function View({ settings }: { settings: boolean }) {
      const scrollContainerRef = useRef<HTMLDivElement>(null);
      return settings ? <p>Settings</p> : <div ref={scrollContainerRef} data-scroller>
        <Grid scrollContainerRef={scrollContainerRef} />
      </div>;
    }
    const view = (settings: boolean) => strict
      ? <StrictMode><View settings={settings} /></StrictMode>
      : <View settings={settings} />;
    const mounted = render(view(false));
    const assertBounded = () => {
      expect(commits.length).toBeGreaterThan(1);
      for (const mountedCount of commits) expect(mountedCount).toBeLessThanOrEqual(65);
      expect(ranges.length).toBeGreaterThan(0);
      for (const range of ranges) {
        expect(range.endIndex - range.startIndex + 1).toBeLessThanOrEqual(65);
        expect(range.visibleEndIndex - range.visibleStartIndex + 1).toBeLessThanOrEqual(25);
        expect(range.visibleStartIndex - range.startIndex).toBeLessThanOrEqual(20);
        expect(range.endIndex - range.visibleEndIndex).toBeLessThanOrEqual(20);
      }
      expect(onReachEnd).not.toHaveBeenCalled();
    };
    assertBounded();
    expect(mounted.container.querySelectorAll("[data-slot]")).toHaveLength(35);
    expect(apiMocks.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([]);

    mounted.rerender(view(true));
    expect(resizeCallbacks.size).toBe(0);
    commits.length = 0;
    ranges.length = 0;
    mounted.rerender(view(false));
    assertBounded();
    expect(mounted.container.querySelectorAll("[data-slot]")).toHaveLength(35);
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(0);

    const scroller = mounted.container.querySelector<HTMLElement>("[data-scroller]");
    if (!scroller) throw new Error("Gallery scroller did not mount");
    scroller.scrollTo = vi.fn((options?: ScrollToOptions | number) => {
      if (typeof options === "object") scroller.scrollTop = options.top ?? 0;
    });
    act(() => {
      scroller.scrollTop = 80 + 198 * 100;
      scroller.dispatchEvent(new Event("scroll"));
    });
    assertBounded();
    expect(ranges[ranges.length - 1]?.visibleStartIndex).toBe(500);
    expect(apiMocks.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([384, 512]);
    width = 584;
    act(() => { for (const callback of resizeCallbacks) callback([]); });
    act(() => { scroller.dispatchEvent(new Event("scroll")); });
    assertBounded();
    expect(scroller.scrollTop).toBe(80 + 166 * 198);
    expect(mounted.container.querySelector('[data-slot="500"]')).not.toBeNull();
    // A narrower grid briefly reports the old scroll offset before anchoring.
    expect(apiMocks.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([384, 512, 256]);
    mounted.unmount();
    expect(resizeCallbacks.size).toBe(0);
  });

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
