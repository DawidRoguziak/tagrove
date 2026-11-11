import { cleanup, fireEvent, render, renderHook, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createLibrary } from "../../../library/createLibrary";
import { MAX_CACHED_PAGES } from "../../../library/query/createAssetQuery";
import { createSelection } from "../createSelection";
import { GalleryGrid } from "../GalleryGrid";
import { MAX_GRID_HEIGHT } from "../gridGeometry";
import { LanguageProvider } from "../../../i18n/solid";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import { idRangeResult, sequentialIds } from "../../../test/idRange";
const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  getAssetQueryIdRange: vi.fn(),
  getAssetQuerySections: vi.fn()
}));
vi.mock("../../../platform/tauri/assets", async () => ({
  ...(await vi.importActual("../../../platform/tauri/assets")),
  ...api,
  toMediaSrc: (path: string) => path
}));
vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn().mockResolvedValue({ items: [], total: 0 })
}));
const V = "0123456789abcdef";
/** `src` of the tile image at a global index. */
const tileSrc = (container: HTMLElement, index: number) =>
  container.querySelector(`button[data-asset-index="${index}"] img`)?.getAttribute("src");
const asset = (index: number): AssetSummary => ({
  id: index + 1,
  file_name: `synthetic-${index}.png`,
  preview_path: index === 0 ? "/synthetic/animated.gif" : null,
  kind: index === 0 ? "gif" : "image",
  modified_at: 0,
  width: 100,
  height: 100,
  duration_ms: null,
  thumb_version: "0123456789abcdef",
  is_favorite: false,
  media_group_key: index < 2 ? "group" : null,
  media_group_order: null
});
const page = (offset: number, total = 26000) => ({
  status: "ready" as const,
  session_id: 1,
  revision: 1,
  offset,
  total,
  items: Array.from({ length: Math.min(128, total - offset) }, (_, i) => asset(offset + i))
});
const observers = new Set<ResizeObserver>();
const resizeCallbacks = new Map<ResizeObserver, ResizeObserverCallback>();
const observed = new Map<ResizeObserver, Set<Element>>();
/** Elements observed while a ResizeObserver callback ran (a browser "loop" error). */
const observedDuringDelivery: Element[] = [];
let delivering = false;
/** Extra px above the grid (a toolbar that grew), measured only by the observers. */
let shiftAbove = 0;
function deliverResize() {
  delivering = true;
  try {
    // Effects run in the delivery, as microtasks do between ResizeObserver callbacks.
    for (const observer of [...observers]) {
      resizeCallbacks.get(observer)?.([], observer);
      flush();
    }
  } finally {
    delivering = false;
  }
}
beforeEach(() => {
  observers.clear();
  resizeCallbacks.clear();
  observed.clear();
  observedDuringDelivery.length = 0;
  shiftAbove = 0;
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class implements ResizeObserver {
      constructor(callback: ResizeObserverCallback) {
        resizeCallbacks.set(this, callback);
      }
      observe(target: Element) {
        observers.add(this);
        observed.set(this, (observed.get(this) ?? new Set()).add(target));
        if (delivering) observedDuringDelivery.push(target);
      }
      unobserve() {}
      disconnect() {
        observers.delete(this);
        observed.delete(this);
      }
    }
  );
  api.startAssetQuery.mockResolvedValue(page(0));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset));
  for (const name of ["clientWidth", "offsetWidth"] as const)
    vi.spyOn(HTMLElement.prototype, name, "get").mockReturnValue(988);
  for (const name of ["clientHeight", "offsetHeight"] as const)
    vi.spyOn(HTMLElement.prototype, name, "get").mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
    this: HTMLElement
  ) {
    // A detached element has an empty box, as in WebKit.
    if (!this.isConnected) return new DOMRect(0, 0, 0, 0);
    const scroller = this.closest<HTMLElement>("[data-scroller]");
    const bannerHeight = document.querySelector('[data-testid="gallery-load-error"]') ? 48 : 0;
    return new DOMRect(
      0,
      this === scroller ? 0 : 80 + shiftAbove + bannerHeight - (scroller?.scrollTop ?? 0),
      988,
      600
    );
  });
  HTMLElement.prototype.scrollTo = function (options?: ScrollToOptions | number) {
    if (typeof options === "object") this.scrollTop = options.top ?? 0;
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function mount(selectionEnabled = false) {
  const { result: library } = renderHook(() => createLibrary());
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => library.query.queryEpoch,
      identityEpoch: () => library.tags.epoch,
      getIdsRange: library.query.getIdsRangeAsync,
      sessionId: library.query.sessionId,
      total: () => library.query.total
    })
  );
  await library.query.refresh();
  flush();
  if (selectionEnabled) selection.toggle();
  const click = vi.fn(),
    range = vi.fn(library.handleVirtualRangeChange);
  const view = render(() => {
    const [scroller, setScroller] = createSignal<HTMLElement>();
    return (
      <LanguageProvider>
        <div ref={setScroller} data-scroller>
          <GalleryGrid
            assets={library.query.assets}
            assetCount={library.query.total}
            getAssetAt={library.query.getAssetAt}
            tileSize={188}
            scroller={scroller}
            onVirtualRangeChange={range}
            onSelect={click}
            hasScanRoots
            selectionModeEnabled={selection.enabled}
            isSelected={selection.isSelected}
            onBulkSelectionInteraction={selection.interact}
            onAddFirstFolder={vi.fn()}
            loadError={selection.error ?? library.query.loadError}
            onLoadRetry={() => {
              void library.query.retryLoad();
              void selection.retry();
            }}
            queryEpoch={library.query.queryEpoch}
            pageFailureEpoch={library.query.pageFailureEpoch}
            sections={library.query.sections}
          />
        </div>
      </LanguageProvider>
    );
  });
  flush();
  const scroller = view.container.querySelector<HTMLElement>("[data-scroller]");
  if (!scroller) throw new Error("Missing scroller");
  const scroll = (index: number) => {
    scroller.scrollTop = 80 + Math.floor(index / 5) * 200;
    fireEvent.scroll(scroller);
    flush();
  };
  return { library, selection, view, scroll, click, range, scroller };
}
it("bounds initial DOM, replaces sparse slots, loads late thumbnail demand and reloads beyond 12 cached pages", async () => {
  const { library, view, scroll, range } = await mount();
  expect(view.container.querySelectorAll("button[data-asset-id]").length).toBeLessThan(70);
  for (const [rangeValue] of range.mock.calls)
    if (rangeValue) expect(rangeValue.endIndex - rangeValue.startIndex + 1).toBeLessThan(70);
  let finish: (result: ReturnType<typeof page>) => void = () => {};
  api.getAssetQueryPage.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      })
  );
  scroll(1000);
  expect(view.container.querySelector('[data-asset-index="1000"]')?.tagName).toBe("DIV");
  await waitFor(() => expect(api.getAssetQueryPage).toHaveBeenCalled());
  finish(page(896));
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="1000"]')).toBeInTheDocument()
  );
  expect(tileSrc(view.container, 1000)).toBe(`thumb://localhost/1001?v=${V}`);
  // Visit more pages than the cache keeps so page 0 must be evicted.
  for (let index = 1536; index < 1536 + (MAX_CACHED_PAGES + 4) * 256; index += 256) {
    scroll(index);
    await waitFor(() =>
      expect(
        view.container.querySelector(`button[data-asset-index="${index}"]`)
      ).toBeInTheDocument()
    );
    expect(library.query.assets.length).toBeLessThanOrEqual(MAX_CACHED_PAGES * 128);
    expect(view.container.querySelectorAll("button[data-asset-id]").length).toBeLessThan(70);
  }
  expect(library.query.getAssetAt(0)).toBeUndefined();
  scroll(0);
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="0"]')).toBeInTheDocument()
  );
  expect(api.getAssetQueryPage).toHaveBeenCalledWith(1, 0, 128);
});
it("keeps GIFs static, updates only their thumbnail DOM and selects the current slot identity", async () => {
  const { view, scroll, click, scroller } = await mount();
  const first = view.container.querySelector<HTMLButtonElement>('button[data-asset-index="0"]');
  if (!first) throw new Error("Missing first tile");
  expect(first.querySelector("img")?.src).not.toContain("animated.gif");
  expect(first).toHaveTextContent("GIF");
  expect(first).toHaveTextContent("Part of a media group");
  // GIF tiles load the static thumbnail by ID, never the original file.
  expect(first.querySelector("img")).toHaveAttribute("src", `thumb://localhost/1?v=${V}`);
  expect(first.querySelector("img")).not.toHaveClass("thumbnail-fade-in");
  fireEvent.click(first);
  expect(click).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1 }));
  const retained = view.container.querySelector('button[data-asset-index="25"]');
  scroller.scrollTop = 180;
  fireEvent.scroll(scroller);
  flush();
  expect(view.container.querySelector('button[data-asset-index="25"]')).toBe(retained);
  scroll(0);
  scroll(1000);
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="1000"]')).toBeInTheDocument()
  );
  const current = view.container.querySelector('button[data-asset-index="1000"]');
  if (!current) throw new Error("Missing replacement tile");
  expect(current.querySelector("img")).not.toHaveClass("thumbnail-fade-in");
  fireEvent.click(current);
  expect(click).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1001 }));
});
it("requests overscan thumbnails at prefetch priority and visible ones without it", async () => {
  const { view, range } = await mount();
  const viewport = range.mock.lastCall?.[0];
  if (!viewport) throw new Error("Missing published range");
  const tiles = [...view.container.querySelectorAll<HTMLElement>("button[data-asset-index]")];
  expect(tiles.some((tile) => Number(tile.dataset.assetIndex) > viewport.visibleEndIndex)).toBe(true);
  for (const tile of tiles) {
    const index = Number(tile.dataset.assetIndex);
    const visible = index >= viewport.visibleStartIndex && index <= viewport.visibleEndIndex;
    expect(tile.querySelector("img")?.getAttribute("src")).toBe(
      `thumb://localhost/${index + 1}?v=${V}${visible ? "" : "&p=1"}`
    );
  }
});

/** Fires a successful load on the tile image at a global index. */
function loadTile(container: HTMLElement, index: number) {
  const image = container.querySelector<HTMLImageElement>(`button[data-asset-index="${index}"] img`);
  if (!image) throw new Error(`Missing tile image ${index}`);
  Object.defineProperty(image, "naturalWidth", { configurable: true, value: 100 });
  fireEvent.load(image);
  flush();
}

const scrollPaused = (container: HTMLElement) => {
  const paused = container
    .querySelector("[data-testid=gallery-grid]")
    ?.getAttribute("data-scroll-paused");
  return paused !== null && paused !== undefined && paused !== "false";
};

it("drops p=1 once when an overscan tile scrolls into view before its thumbnail loaded", async () => {
  const { view, range, scroller } = await mount();
  const pending = (range.mock.lastCall?.[0]?.visibleEndIndex ?? -2) + 1;
  const loaded = pending + 1;
  const url = (index: number, prefetch: boolean) =>
    `thumb://localhost/${index + 1}?v=${V}${prefetch ? "&p=1" : ""}`;
  expect(tileSrc(view.container, pending)).toBe(url(pending, true));
  expect(tileSrc(view.container, loaded)).toBe(url(loaded, true));
  loadTile(view.container, loaded);
  // One row down, below the fast-scroll threshold: the first overscan row becomes visible.
  scroller.scrollTop = 280;
  fireEvent.scroll(scroller);
  flush();
  const moved = range.mock.lastCall?.[0];
  expect(moved?.visibleStartIndex).toBeLessThanOrEqual(pending);
  expect(moved?.visibleEndIndex).toBeGreaterThanOrEqual(loaded);
  expect(scrollPaused(view.container)).toBe(false);
  // The new URL moves the queued prefetch render ahead of every prefetch.
  expect(tileSrc(view.container, pending)).toBe(url(pending, false));
  // A loaded picture keeps its URL, so it is never requested again.
  expect(tileSrc(view.container, loaded)).toBe(url(loaded, true));
  // One way only: back in the overscan, the tile keeps the visible URL.
  scroller.scrollTop = 0;
  fireEvent.scroll(scroller);
  flush();
  expect(range.mock.lastCall?.[0]?.visibleEndIndex).toBeLessThan(pending);
  expect(tileSrc(view.container, pending)).toBe(url(pending, false));
  expect(tileSrc(view.container, loaded)).toBe(url(loaded, true));
});

it("keeps p=1 on a pending tile that reaches the view during a fast-scroll pause until it ends", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const { view, range, scroller } = await mount();
    const pending = (range.mock.lastCall?.[0]?.visibleEndIndex ?? -2) + 1;
    const prefetchUrl = `thumb://localhost/${pending + 1}?v=${V}&p=1`;
    expect(tileSrc(view.container, pending)).toBe(prefetchUrl);
    // Two rows in one move start the pause; the overscan row is now visible.
    scroller.scrollTop = 480;
    fireEvent.scroll(scroller);
    flush();
    expect(scrollPaused(view.container)).toBe(true);
    const moved = range.mock.lastCall?.[0];
    expect(moved?.visibleStartIndex).toBeLessThanOrEqual(pending);
    expect(moved?.visibleEndIndex).toBeGreaterThanOrEqual(pending);
    expect(tileSrc(view.container, pending)).toBe(prefetchUrl);
    vi.advanceTimersByTime(150);
    flush();
    expect(scrollPaused(view.container)).toBe(false);
    expect(tileSrc(view.container, pending)).toBe(`thumb://localhost/${pending + 1}?v=${V}`);
  } finally {
    vi.useRealTimers();
  }
});
it("holds failed pages until explicit retry and replaces error placeholders on success", async () => {
  const { view, scroll } = await mount();
  api.getAssetQueryPage.mockRejectedValue(new Error("synthetic page failure"));
  scroll(1000);
  await screen.findByTestId("gallery-load-error");
  const calls = api.getAssetQueryPage.mock.calls.length;
  flush();
  expect(api.getAssetQueryPage).toHaveBeenCalledTimes(calls);
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset));
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  flush();
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="1000"]')).toBeInTheDocument()
  );
  expect(screen.queryByTestId("gallery-load-error")).not.toBeInTheDocument();
});

it.each([
  "shift",
  "rectangle",
  "rectangle-replace"
])("retries the original failed %s selection after the error banner moves the grid", async (kind) => {
  const { library, selection, view, scroll } = await mount(true);
  // Keep an existing ID outside the range as well as the range anchor.
  selection.interact({
    type: "click",
    assetId: 9001,
    assetIndex: 9000,
    shift: false,
    ctrlLike: false
  });
  const anchor = view.container.querySelector('button[data-asset-index="0"]');
  if (!anchor) throw new Error("Missing anchor");
  fireEvent.click(anchor);
  flush();
  for (let index = 256; index <= 4096; index += 256) {
    scroll(index);
    await waitFor(() =>
      expect(
        view.container.querySelector(`button[data-asset-index="${index}"]`)
      ).toBeInTheDocument()
    );
  }
  expect(library.query.getAssetAt(0)).toBeUndefined();
  expect([...selection.ids]).toEqual([9001, 1]);
  api.getAssetQueryIdRange.mockRejectedValue(new Error("synthetic offscreen range failure"));
  const grid = view.container.querySelector('button[data-asset-index="4096"]')?.parentElement;
  if (!grid) throw new Error("Missing measured grid");
  const originalTop = grid.getBoundingClientRect().top;
  if (kind === "shift") {
    const target = view.container.querySelector('button[data-asset-index="4096"]');
    if (!target) throw new Error("Missing range endpoint");
    fireEvent.click(target, { shiftKey: true, ctrlKey: true });
  } else {
    // The released rectangle sends fixed global ranges through the same callback.
    await selection.interact({
      type: "rectangle-commit",
      ranges: [{ startIndex: 0, endIndex: 4096 }],
      additive: kind !== "rectangle-replace"
    });
  }
  await screen.findByTestId("gallery-load-error");
  await waitFor(() => expect(selection.busy).toBe(false));
  const before = api.getAssetQueryIdRange.mock.calls.length;
  expect(before).toBe(1);
  expect(grid.getBoundingClientRect().top).toBe(originalTop + 48);
  // Deliver the production virtualizer's measurement observer after the real banner renders.
  deliverResize();
  expect(selection.error).toContain("synthetic offscreen range failure");
  expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  expect(grid.getBoundingClientRect().top).toBe(originalTop + 48);
  expect(api.getAssetQueryIdRange).toHaveBeenCalledTimes(before);
  expect([...selection.ids]).toEqual([9001, 1]);
  api.getAssetQueryIdRange.mockImplementation(async (_session, start: number, end: number) =>
    idRangeResult(sequentialIds(start, end))
  );
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  const expectedIds = Array.from({ length: 4097 }, (_, i) => i + 1);
  if (kind !== "rectangle-replace") expectedIds.unshift(9001);
  await waitFor(() => expect([...selection.ids]).toEqual(expectedIds));
  // Retry repeats the original whole range in one binary read.
  expect(api.getAssetQueryIdRange.mock.calls.slice(before)).toEqual([[1, 0, 4096]]);
  expect(screen.queryByTestId("gallery-load-error")).not.toBeInTheDocument();
  deliverResize();
  expect([...selection.ids]).toEqual(expectedIds);
});


it("demands the final million-result page and passes the required total to the idle footer", async () => {
  api.startAssetQuery.mockResolvedValue(page(0, 1_000_000));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset, 1_000_000));
  const { library, view, scroller, click } = await mount();
  // A million results exceed the physical window; End moves it to the final rows.
  expect(view.container.querySelector('[data-testid="gallery-grid-content"]')).toHaveStyle({
    height: `${MAX_GRID_HEIGHT}px`
  });
  fireEvent.keyDown(scroller, { key: "End" });
  flush();
  await waitFor(() => expect(view.container.querySelector('button[data-asset-index="999999"]')).toBeInTheDocument());
  expect(api.getAssetQueryPage).toHaveBeenCalledWith(1, 999936, 128);
  // Near-end notification must never fetch the old sequential offset after resume.
  expect(api.getAssetQueryPage.mock.calls.every(([, offset]) => offset >= 999808)).toBe(true);
  expect(library.query.assets.length).toBeLessThanOrEqual(MAX_CACHED_PAGES * 128);
  expect(view.container.querySelectorAll("[data-asset-index]").length).toBeLessThan(70);
  expect(screen.getByTestId("gallery-end-state")).toHaveTextContent("1,000,000 items");
  const last = view.container.querySelector('button[data-asset-index="999999"]');
  if (!last) throw new Error("Missing final tile");
  fireEvent.click(last);
  expect(click).toHaveBeenCalledWith(expect.objectContaining({id: 1_000_000}));
});

it("renders static slots without thumbnail or passing-page requests and resumes the unchanged final range", async () => {
  const { view, scroll } = await mount();
  expect(tileSrc(view.container, 0)).toBe(`thumb://localhost/1?v=${V}`);
  scroll(1000);
  scroll(2000);
  scroll(3000);
  expect(view.container.querySelectorAll("[data-asset-index]").length).toBeGreaterThan(0);
  // Unloaded slots are static placeholders: no image request, no pulse.
  expect(view.container.querySelectorAll("img, .animate-pulse")).toHaveLength(0);
  expect(api.getAssetQueryPage).not.toHaveBeenCalled();
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="3000"]')).toBeInTheDocument()
  );
  expect(api.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([2944]);
  // Loaded after the pause: the rendering URL, not the cached-only one.
  expect(tileSrc(view.container, 3000)).toBe(`thumb://localhost/3001?v=${V}`);
});

const rowOrder = (container: HTMLElement) =>
  [...container.querySelectorAll<HTMLElement>("[data-gallery-row]:not([hidden])")].map((row) =>
    Number(row.dataset.galleryRow)
  );

it("keeps loaded tiles mounted through a fast jump and suspends only page and thumbnail demand", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const { view, scroll } = await mount();
    const before = new Set(view.container.querySelectorAll("button[data-asset-index]"));
    expect(before.size).toBe(60);
    api.getAssetQueryPage.mockClear();
    // Twelve rows down stays inside the first cached page (indices 0-127).
    scroll(60);
    // Same reading as the E2E specs: present and not "false".
    const paused = view.container
      .querySelector("[data-testid=gallery-grid]")
      ?.getAttribute("data-scroll-paused");
    expect(paused === null || paused === "false").toBe(false);
    const during = view.container.querySelectorAll("button[data-asset-index]");
    expect(new Set(during)).toEqual(before);
    expect(view.container.querySelector('button[data-asset-index="60"]')).toBeInTheDocument();
    expect(view.container.querySelectorAll("img")).toHaveLength(60);
    expect(view.container.querySelectorAll(".animate-pulse")).toHaveLength(0);
    expect(api.getAssetQueryPage).not.toHaveBeenCalled();
    // Assets first shown during the pause ask for cached thumbnails only.
    expect(tileSrc(view.container, 60)).toBe(`thumb://localhost/61?v=${V}&c=1`);
    expect(tileSrc(view.container, 0)).toBeUndefined();
    vi.advanceTimersByTime(150);
    flush();
    // A cached-only request that hit keeps its URL after the pause (no reload).
    expect(tileSrc(view.container, 60)).toBe(`thumb://localhost/61?v=${V}&c=1`);
    // A miss switches to the rendering URL once the pause is over.
    const missed = view.container.querySelector('button[data-asset-index="60"] img');
    if (!missed) throw new Error("Missing tile image");
    fireEvent.error(missed);
    flush();
    expect(tileSrc(view.container, 60)).toBe(`thumb://localhost/61?v=${V}`);
  } finally {
    vi.useRealTimers();
  }
});

it("recycles one row slot per crossed row with a constant DOM and visual row order", async () => {
  const { view, scroll } = await mount();
  scroll(50);
  const content = view.container.querySelector<HTMLElement>('[data-testid="gallery-grid-content"]');
  if (!content) throw new Error("Missing grid content");
  const nodes = content.querySelectorAll("*").length;
  const rows = [...content.querySelectorAll<HTMLElement>("[data-gallery-row]")];
  const tiles = new Set(content.querySelectorAll("button[data-asset-index]"));
  expect(rowOrder(content)).toEqual([6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]);
  scroll(55);
  expect(rowOrder(content)).toEqual([7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]);
  expect(content.querySelectorAll("*").length).toBe(nodes);
  expect(new Set(content.querySelectorAll("[data-gallery-row]"))).toEqual(new Set(rows));
  // The recycled row moved to the end and kept its tile elements.
  expect(new Set(content.querySelectorAll("button[data-asset-index]"))).toEqual(tiles);
  expect(content.querySelector('button[data-asset-index="94"]')?.closest("[data-gallery-row]")).toBe(
    rows[0]
  );
  expect(content.lastElementChild).toBe(rows[0]);
});

it("draws media-group backplates per row behind adjacent grouped tiles", async () => {
  const { view } = await mount();
  const row = view.container.querySelector('[data-gallery-row="0"]');
  const plates = row?.querySelectorAll<HTMLElement>("[data-gallery-group-plate]") ?? [];
  expect(plates).toHaveLength(1);
  expect(plates[0].style.width).toBe(`${2 * 188 + 12 + 8}px`);
  expect(plates[0].style.left).toBe("2px");
  expect(view.container.querySelectorAll("[data-gallery-group-plate]")).toHaveLength(1);
});

it("adds a keyboard scrollbar only for results taller than the physical window", async () => {
  const small = await mount();
  expect(screen.queryByRole("scrollbar")).toBeNull();
  small.view.unmount();
  api.startAssetQuery.mockResolvedValue(page(0, 1_000_000));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset, 1_000_000));
  const { view } = await mount();
  const bar = screen.getByRole("scrollbar", { name: "Gallery position" });
  expect(bar).toHaveAttribute("aria-valuemax", "1000000");
  expect(bar).toHaveAttribute("aria-valuenow", "1");
  expect(bar).toHaveAttribute("aria-controls", "gallery-grid-content");
  fireEvent.keyDown(bar, { key: "End" });
  flush();
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="999999"]')).toBeInTheDocument()
  );
  expect(Number(bar.getAttribute("aria-valuenow"))).toBeGreaterThan(999_900);
  fireEvent.keyDown(bar, { key: "Home" });
  flush();
  expect(bar).toHaveAttribute("aria-valuenow", "1");
});

it("adds the month timeline beside the position bar and jumps to the oldest month", async () => {
  api.getAssetQuerySections.mockResolvedValue({
    status: "ready",
    session_id: 1,
    total: 26000,
    sections: [{ t: 202403, start: 0, count: 26000 }]
  });
  const single = await mount();
  await waitFor(() => expect(api.getAssetQuerySections).toHaveBeenCalledWith(1));
  // One month is no timeline.
  expect(screen.queryByRole("slider")).toBeNull();
  single.view.unmount();
  const total = 1_000_000;
  api.startAssetQuery.mockResolvedValue(page(0, total));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset, total));
  api.getAssetQuerySections.mockResolvedValue({
    status: "ready",
    session_id: 1,
    total,
    sections: [
      { t: 202403, start: 0, count: 600_000 },
      { t: 201507, start: 600_000, count: 399_990 },
      { t: 0, start: 999_990, count: 10 }
    ]
  });
  const { view } = await mount();
  const slider = await screen.findByRole("slider", { name: "Timeline" });
  expect(screen.getByRole("scrollbar", { name: "Gallery position" })).toBeInTheDocument();
  expect(slider).toHaveAttribute("aria-valuetext", "March 2024");
  fireEvent.keyDown(slider, { key: "ArrowDown" });
  flush();
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="600000"]')).toBeInTheDocument()
  );
  expect(slider).toHaveAttribute("aria-valuetext", "July 2015");
  fireEvent.keyDown(slider, { key: "End" });
  flush();
  expect(slider).toHaveAttribute("aria-valuetext", "No date");
});

it("keeps a deep viewport's tiles mounted while a refresh reloads its pages", async () => {
  api.startAssetQuery.mockResolvedValue(page(0));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset));
  const { library, view, scroll } = await mount();
  scroll(5000);
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="5000"]')).toBeInTheDocument()
  );
  const reloads: Array<{ offset: number; resolve: (value: ReturnType<typeof page>) => void }> = [];
  api.getAssetQueryPage.mockImplementation(
    (_session, offset) => new Promise((resolve) => reloads.push({ offset, resolve }))
  );
  await library.query.refresh();
  flush();
  // The first page of the new query does not cover the viewport; old rows stay as fallback.
  expect(view.container.querySelector('button[data-asset-index="5000"]')).toBeInTheDocument();
  await waitFor(() => expect(reloads.length).toBeGreaterThan(0));
  for (const reload of reloads) reload.resolve(page(reload.offset));
  await waitFor(() =>
    expect(library.query.getAssetIndex(5001)).toBe(5000)
  );
  expect(view.container.querySelector('button[data-asset-index="5000"]')).toBeInTheDocument();
});

it("enters the empty-result state without observing the detached grid or observing during a delivery", async () => {
  const { library, view } = await mount();
  deliverResize();
  expect(observedDuringDelivery).toEqual([]);
  const content = view.container.querySelector('[data-testid="gallery-grid-content"]');
  if (!content) throw new Error("Missing grid content");
  api.startAssetQuery.mockResolvedValue(page(0, 0));
  await library.query.refresh();
  flush();
  expect(await screen.findByText("No results")).toBeInTheDocument();
  expect(content.isConnected).toBe(false);
  const targets = [...observed.values()].flatMap((set) => [...set]);
  expect(targets).not.toContain(content);
  expect(targets.every((target) => target.isConnected)).toBe(true);
  // The browser reports the unmounted grid's size change in the next frame.
  deliverResize();
  expect(observedDuringDelivery).toEqual([]);
  // The grid comes back measured, with its observers attached again.
  api.startAssetQuery.mockResolvedValue(page(0));
  await library.query.refresh();
  flush();
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="0"]')).toBeInTheDocument()
  );
  const next = view.container.querySelector('[data-testid="gallery-grid-content"]');
  expect([...observed.values()].some((set) => next !== null && set.has(next))).toBe(true);
});

it("publishes a grid margin change from its ResizeObserver without observing in that delivery", async () => {
  const { view } = await mount();
  const content = view.container.querySelector<HTMLElement>('[data-testid="gallery-grid-content"]');
  const row = content?.querySelector<HTMLElement>('[data-gallery-row="0"]');
  if (!row) throw new Error("Missing first row");
  shiftAbove = 40;
  deliverResize();
  expect(observedDuringDelivery).toEqual([]);
  // The rows stay in place: the margin moves the grid, not the scroll position.
  expect(row.style.transform).toBe("translate(0, -6px)");
});

it("lets a recycled tile's late image load or error after a large jump pass without a stale Show read", async () => {
  const errors: string[] = [];
  const onError = (event: ErrorEvent) => {
    event.preventDefault();
    errors.push(event.message);
  };
  window.addEventListener("error", onError);
  try {
    const { view, scroll } = await mount();
    const images = [...view.container.querySelectorAll("img")];
    expect(images.length).toBe(60);
    // Every row slot moves to unloaded indices: the tiles become placeholders.
    scroll(5000);
    expect(view.container.querySelector('div[data-asset-index="5000"]')).toBeInTheDocument();
    expect(images.every((image) => !image.isConnected)).toBe(true);
    // WebKit still answers the detached images' thumbnail requests.
    for (const image of images) {
      fireEvent.load(image);
      fireEvent.error(image);
    }
    flush();
    expect(errors).toEqual([]);
    await waitFor(() =>
      expect(view.container.querySelector('button[data-asset-index="5000"]')).toBeInTheDocument()
    );
    expect(tileSrc(view.container, 5000)).toBe(`thumb://localhost/5001?v=${V}`);
  } finally {
    window.removeEventListener("error", onError);
  }
});
