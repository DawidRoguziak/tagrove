import { cleanup, fireEvent, render, renderHook, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createLibrary } from "../../../library/createLibrary";
import { createSelection } from "../createSelection";
import { GalleryGrid } from "../GalleryGrid";
import { MAX_GRID_HEIGHT } from "../gridGeometry";
import { LanguageProvider } from "../../../i18n/solid";
import { ThumbnailStore } from "../../../library/thumbnails/thumbnailStore";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  demand: vi.fn()
}));
vi.mock("../../../platform/tauri/assets", async () => ({
  ...(await vi.importActual("../../../platform/tauri/assets")),
  ...api,
  toMediaSrc: (path: string) => path
}));
vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn().mockResolvedValue({ items: [], total: 0 })
}));
vi.mock("../../../library/thumbnails/createThumbnailQueue", () => ({
  createThumbnailQueue: () => ({
    thumbnailStore: new ThumbnailStore(),
    resetThumbnailQueue: vi.fn(),
    setGalleryThumbnailDemand: api.demand,
    queueThumbnailsByIds: vi.fn(),
    isGeneratingPage: false,
    pendingPageSize: 0
  })
}));
const asset = (index: number): AssetSummary => ({
  id: index + 1,
  file_name: `synthetic-${index}.png`,
  preview_path: index === 0 ? "/synthetic/animated.gif" : null,
  kind: index === 0 ? "gif" : "image",
  modified_at: 0,
  width: 100,
  height: 100,
  duration_ms: null,
  thumb_path: null,
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
function deliverResize() {
  for (const observer of [...observers]) resizeCallbacks.get(observer)?.([], observer);
  flush();
}
beforeEach(() => {
  observers.clear();
  resizeCallbacks.clear();
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class implements ResizeObserver {
      constructor(callback: ResizeObserverCallback) {
        resizeCallbacks.set(this, callback);
      }
      observe() {
        observers.add(this);
      }
      unobserve() {}
      disconnect() {
        observers.delete(this);
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
    const scroller = this.closest<HTMLElement>("[data-scroller]");
    const bannerHeight = document.querySelector('[data-testid="gallery-load-error"]') ? 48 : 0;
    return new DOMRect(
      0,
      this === scroller ? 0 : 80 + bannerHeight - (scroller?.scrollTop ?? 0),
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
async function mount(selectionEnabled = false, generating = false) {
  const { result: library } = renderHook(() => createLibrary());
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => library.query.queryEpoch,
      identityEpoch: () => library.tags.epoch,
      getIdsRange: library.query.getIdsRangeAsync
    })
  );
  await library.query.refresh();
  flush();
  if (selectionEnabled) selection.toggle();
  let subscriptions = 0;
  const store = library.thumbnails.thumbnailStore;
  const subscribe = store.subscribe;
  vi.spyOn(store, "subscribe").mockImplementation((id, listener) => {
    subscriptions++;
    const unsubscribe = subscribe(id, listener);
    return () => { subscriptions--; unsubscribe(); };
  });
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
            hasMore={library.query.offset < library.query.total}
            isLoading={library.query.loading}
            isGeneratingThumbnails={generating}
            pendingThumbnailCount={3}
            thumbnailStore={library.thumbnails.thumbnailStore}
            scroller={scroller}
            onReachEnd={library.handleGalleryReachEnd}
            onVirtualRangeChange={range}
            onSelect={click}
            hasScanRoots
            selectionModeEnabled={selection.enabled}
            selectedAssetIds={selection.ids}
            onBulkSelectionInteraction={selection.interact}
            onAddFirstFolder={vi.fn()}
            loadError={selection.error ?? library.query.loadError}
            onLoadRetry={() => {
              void library.query.retryLoad();
              void selection.retry();
            }}
            queryEpoch={library.query.queryEpoch}
            pageFailureEpoch={library.query.pageFailureEpoch}
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
  return { library, selection, view, scroll, click, range, scroller, subscriptions: () => subscriptions };
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
  expect(api.demand.mock.calls.some(([visible]) => visible.includes(1001))).toBe(true);
  for (let index = 1536; index < 4096; index += 256) {
    scroll(index);
    await waitFor(() =>
      expect(
        view.container.querySelector(`button[data-asset-index="${index}"]`)
      ).toBeInTheDocument()
    );
    expect(library.query.assets.length).toBeLessThanOrEqual(1536);
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
  const { library, view, scroll, click, range, scroller } = await mount();
  const first = view.container.querySelector<HTMLButtonElement>('button[data-asset-index="0"]');
  if (!first) throw new Error("Missing first tile");
  expect(first.querySelector("img")?.src).not.toContain("animated.gif");
  expect(first).toHaveTextContent("GIF");
  expect(first).toHaveTextContent("Part of a media group");
  const unchanged = view.container.querySelector('button[data-asset-index="1"] img');
  range.mockClear();
  library.thumbnails.thumbnailStore.complete({ 1: "/synthetic/still.jpg" }, [1]);
  flush();
  expect(first.querySelector("img")).toHaveAttribute("src", "/synthetic/still.jpg");
  expect(first.querySelector("img")).not.toHaveClass("thumbnail-fade-in");
  expect(view.container.querySelector('button[data-asset-index="1"] img')).toBe(unchanged);
  expect(range).not.toHaveBeenCalled();
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
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => {
    if (offset === 0) throw new Error("synthetic offscreen page failure");
    return page(offset);
  });
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
  const before = api.getAssetQueryPage.mock.calls.length;
  expect(grid.getBoundingClientRect().top).toBe(originalTop + 48);
  // Deliver the production virtualizer's measurement observer after the real banner renders.
  deliverResize();
  expect(selection.error).toContain("synthetic offscreen page failure");
  expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  expect(grid.getBoundingClientRect().top).toBe(originalTop + 48);
  expect(api.getAssetQueryPage).toHaveBeenCalledTimes(before);
  expect([...selection.ids]).toEqual([9001, 1]);
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset));
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  const expectedIds = Array.from({ length: 4097 }, (_, i) => i + 1);
  if (kind !== "rectangle-replace") expectedIds.unshift(9001);
  await waitFor(() => expect([...selection.ids]).toEqual(expectedIds));
  expect(api.getAssetQueryPage.mock.calls.slice(before).some(([, offset]) => offset === 0)).toBe(
    true
  );
  expect(screen.queryByTestId("gallery-load-error")).not.toBeInTheDocument();
  deliverResize();
  expect([...selection.ids]).toEqual(expectedIds);
});


it("demands the final million-result page and passes the required total to the idle footer", async () => {
  api.startAssetQuery.mockResolvedValue(page(0, 1_000_000));
  api.getAssetQueryPage.mockImplementation(async (_session, offset) => page(offset, 1_000_000));
  const { library, view, scroller, click } = await mount();
  expect(screen.queryByTestId("gallery-end-state")).toBeNull();
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
  expect(library.query.assets.length).toBeLessThanOrEqual(1536);
  expect(view.container.querySelectorAll("[data-asset-index]").length).toBeLessThan(70);
  expect(screen.getByTestId("gallery-end-state")).toHaveTextContent("1,000,000 items");
  const last = view.container.querySelector('button[data-asset-index="999999"]');
  if (!last) throw new Error("Missing final tile");
  fireEvent.click(last);
  expect(click).toHaveBeenCalledWith(expect.objectContaining({id: 1_000_000}));
});

it("shows thumbnail progress only while the query still has more rows", async () => {
  const { library, scroll } = await mount(false, true);
  expect(screen.getByRole("status")).toHaveTextContent("Generating thumbnails for 3 queued items.");
  scroll(25999);
  await waitFor(() => expect(library.query.offset).toBe(26000));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByTestId("gallery-end-state")).toBeNull();
});


it("renders static slots without subscriptions or passing-page requests and resumes the unchanged final range", async () => {
  const { view, scroll, library, subscriptions } = await mount();
  expect(subscriptions()).toBeGreaterThan(0);
  scroll(1000);
  scroll(2000);
  scroll(3000);
  expect(view.container.querySelectorAll("[data-asset-index]").length).toBeGreaterThan(0);
  expect(view.container.querySelectorAll("img, .animate-pulse, .animate-spin")).toHaveLength(0);
  expect(subscriptions()).toBe(0);
  expect(api.getAssetQueryPage).not.toHaveBeenCalled();
  expect(api.demand).toHaveBeenLastCalledWith([], []);
  library.thumbnails.thumbnailStore.complete({ 1: "/synthetic/late.jpg" }, [1]);
  flush();
  expect(subscriptions()).toBe(0);
  await waitFor(() => expect(view.container.querySelector('button[data-asset-index="3000"]')).toBeInTheDocument());
  expect(api.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([2944]);
  expect(subscriptions()).toBeGreaterThan(0);
  expect(api.demand.mock.lastCall?.[0]).toContain(3001);
  view.unmount(); flush();
  expect(subscriptions()).toBe(0);
  expect(api.demand).toHaveBeenLastCalledWith([], []);
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
    api.demand.mockClear();
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
    expect(api.demand).toHaveBeenLastCalledWith([], []);
    vi.advanceTimersByTime(150);
    flush();
    expect(api.demand.mock.lastCall?.[0]).toContain(61);
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
