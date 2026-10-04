import { cleanup, fireEvent, render, renderHook, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createLibrary } from "../../../library/createLibrary";
import { GalleryGrid } from "../GalleryGrid";
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
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
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
    return new DOMRect(0, this === scroller ? 0 : 80 - (scroller?.scrollTop ?? 0), 988, 600);
  });
  HTMLElement.prototype.scrollTo = function (options?: ScrollToOptions | number) {
    if (typeof options === "object") this.scrollTop = options.top ?? 0;
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function mount() {
  const { result: library } = renderHook(() => createLibrary());
  await library.query.refresh();
  flush();
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
            isGeneratingThumbnails={false}
            pendingThumbnailCount={0}
            thumbnailStore={library.thumbnails.thumbnailStore}
            scroller={scroller}
            onReachEnd={library.query.handleReachEnd}
            onVirtualRangeChange={range}
            onSelect={click}
            hasScanRoots
            selectionModeEnabled={false}
            selectedAssetIds={new Set()}
            onBulkSelectionInteraction={vi.fn()}
            onAddFirstFolder={vi.fn()}
            loadError={library.query.loadError}
            onLoadRetry={library.query.retryLoad}
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
  return { library, view, scroll, click, range };
}
it("bounds initial DOM, replaces sparse slots, loads late thumbnail demand and reloads beyond 12 cached pages", async () => {
  const { library, view, scroll, range } = await mount();
  expect(view.container.querySelectorAll("button[data-asset-id]").length).toBeLessThan(70);
  for (const [rangeValue] of range.mock.calls)
    expect(rangeValue.endIndex - rangeValue.startIndex + 1).toBeLessThan(70);
  let finish: (result: ReturnType<typeof page>) => void = () => {};
  api.getAssetQueryPage.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      })
  );
  scroll(1000);
  expect(view.container.querySelector('[data-asset-index="1000"]')?.tagName).toBe("DIV");
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
  const { library, view, scroll, click, range } = await mount();
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
  expect(view.container.querySelector('button[data-asset-index="1"] img')).toBe(unchanged);
  expect(range).not.toHaveBeenCalled();
  fireEvent.click(first);
  expect(click).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1 }));
  const retained = view.container.querySelector('button[data-asset-index="25"]');
  scroll(30);
  expect(view.container.querySelector('button[data-asset-index="25"]')).toBe(retained);
  scroll(0);
  scroll(1000);
  await waitFor(() =>
    expect(view.container.querySelector('button[data-asset-index="1000"]')).toBeInTheDocument()
  );
  const current = view.container.querySelector('button[data-asset-index="1000"]');
  if (!current) throw new Error("Missing replacement tile");
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
