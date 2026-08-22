import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../types";
import { GalleryGrid } from "../GalleryGrid";

vi.mock("../../../api", () => ({
  toMediaSrc: (path: string) => `media://${path}`
}));

let virtualItems: Array<{ key: number; index: number; start: number; lane?: number; end?: number }> = [];

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: () => ({
    getVirtualItems: () => virtualItems,
    getTotalSize: () => 800
  })
}));

const resizeObservers: TestResizeObserver[] = [];

class TestResizeObserver implements ResizeObserver {
  private callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    resizeObservers.push(this);
  }

  disconnect(): void {}
  observe(): void {}
  unobserve(): void {}

  trigger(width: number, height: number): void {
    const entry = {
      contentRect: { width, height }
    } as ResizeObserverEntry;
    this.callback([entry], this as unknown as ResizeObserver);
  }
}

const sampleAsset: AssetSummary = {
  id: 7,
  file_name: "a.jpg",
  preview_path: null,
  kind: "image",
  modified_at: 1700000000,
  width: 1200,
  height: 800,
  duration_ms: null,
  thumb_path: null,
  is_favorite: false,
  media_group_key: null,
  media_group_order: null
};

const sampleGifAsset: AssetSummary = {
  ...sampleAsset,
  id: 8,
  file_name: "a.gif",
  preview_path: "C:/media/a.gif",
  kind: "gif"
};

const transparentThumbnailSrc =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

function createGifAssets(count: number): AssetSummary[] {
  return Array.from({ length: count }, (_, index) => ({
    ...sampleGifAsset,
    id: index + 1,
    file_name: `gif-${index + 1}.gif`,
    preview_path: `C:/media/gif-${index + 1}.gif`
  }));
}

function createGifThumbs(assets: AssetSummary[]): Record<number, string> {
  return Object.fromEntries(
    assets.map((asset, index) => [asset.id, `C:/thumbs/gif-${index + 1}.jpg`])
  );
}

describe("GalleryGrid", () => {
  const originalResizeObserver = globalThis.ResizeObserver;

  beforeEach(() => {
    resizeObservers.length = 0;
    virtualItems = [];
    vi.stubGlobal("ResizeObserver", TestResizeObserver);
  });

  afterEach(() => {
    cleanup();
    vi.stubGlobal("ResizeObserver", originalResizeObserver as typeof ResizeObserver);
  });

  it("loads more after gallery resize when list still does not scroll", async () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const onReachEnd = vi.fn();
    const { getByTestId } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={onReachEnd}
        onSelect={() => {}}
      />
    );

    const gallery = getByTestId("gallery-grid") as HTMLDivElement;
    Object.defineProperty(gallery, "clientHeight", { value: 700, configurable: true });
    Object.defineProperty(gallery, "scrollHeight", { value: 600, configurable: true });

    await waitFor(() => {
      expect(onReachEnd).toHaveBeenCalled();
    });

    onReachEnd.mockClear();

    await act(async () => {
      resizeObservers[0]?.trigger(1400, 900);
    });

    await waitFor(() => {
      expect(onReachEnd).toHaveBeenCalledTimes(1);
    });
  });

  it("shows GIF badge on gif tiles", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByText } = render(
      <GalleryGrid
        assets={[sampleGifAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    const gifBadge = getByText("GIF");
    expect(gifBadge).toBeInTheDocument();
    expect(gifBadge).toHaveClass("right-0", "top-0");
    expect(gifBadge).not.toHaveClass("left-2");
  });

  it("shows a query error and delegates Retry", () => {
    const onLoadRetry = vi.fn();
    const { getByRole, getByTestId } = render(
      <GalleryGrid
        assets={[]}
        selectedId={null}
        thumbs={{}}
        tileSize={180}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
        loadError="offline"
        onLoadRetry={onLoadRetry}
      />
    );

    expect(getByTestId("gallery-load-error")).toHaveTextContent("Loading the library failed.");
    fireEvent.click(getByRole("button", { name: "Retry" }));
    expect(onLoadRetry).toHaveBeenCalledTimes(1);
  });

  it("eagerly loads mounted virtualized thumbnails", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByAltText } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{ 7: "C:/thumbs/a.jpg" }}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    const preview = getByAltText("a.jpg");
    expect(preview).toHaveAttribute("src", "media://C:/thumbs/a.jpg");
    expect(preview).not.toHaveAttribute("loading");
  });

  it("shows a transparent fallback when a thumbnail fails to load", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByAltText } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{ 7: "C:/thumbs/a.jpg" }}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    const preview = getByAltText("a.jpg");
    fireEvent.error(preview);

    expect(preview).toHaveAttribute("src", transparentThumbnailSrc);
  });

  it("retries loading after thumbPath changes following an error", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByAltText, rerender } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{ 7: "C:/thumbs/a.jpg" }}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    fireEvent.error(getByAltText("a.jpg"));

    rerender(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{ 7: "C:/thumbs/a-retry.jpg" }}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    expect(getByAltText("a.jpg")).toHaveAttribute(
      "src",
      "media://C:/thumbs/a-retry.jpg"
    );
  });

  it("animates gifs when visible gif count is within threshold", () => {
    const gifAssets = createGifAssets(10);
    const thumbs = createGifThumbs(gifAssets);
    virtualItems = gifAssets.map((_, index) => ({ key: index, index, start: -1, end: 1 }));

    const { getByAltText } = render(
      <GalleryGrid
        assets={gifAssets}
        selectedId={null}
        thumbs={thumbs}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    expect(getByAltText("C:/media/gif-1.gif")).toHaveAttribute("src", "media://C:/media/gif-1.gif");
  });

  it("does not animate gifs when visible gif count is above threshold", () => {
    const gifAssets = createGifAssets(11);
    const thumbs = createGifThumbs(gifAssets);
    virtualItems = gifAssets.map((_, index) => ({ key: index, index, start: -1, end: 1 }));

    const { getByAltText } = render(
      <GalleryGrid
        assets={gifAssets}
        selectedId={null}
        thumbs={thumbs}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    expect(getByAltText("C:/media/gif-1.gif")).toHaveAttribute("src", "media://C:/thumbs/gif-1.jpg");
  });

  it("shows thumbnail generation loader while waiting for next page", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByText } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore
        isLoading={false}
        isGeneratingThumbnails
        pendingThumbnailCount={12}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    expect(
      getByText("Generating thumbnails for 12 queued items.")
    ).toBeInTheDocument();
  });

  it("shows loader on tile when thumbnail is rendering", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];

    const { getByTestId } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore
        isLoading={false}
        isGeneratingThumbnails
        pendingThumbnailCount={1}
        renderingThumbnailIds={{ 7: true }}
        onReachEnd={() => {}}
        onSelect={() => {}}
      />
    );

    expect(getByTestId("tile-thumb-loader")).toBeInTheDocument();
  });

  it("reports virtual range to parent callback", async () => {
    virtualItems = [
      { key: 1, index: 3, start: 0 },
      { key: 2, index: 9, start: 188 }
    ];
    const assets = Array.from({ length: 20 }, (_, index) => ({
      ...sampleAsset,
      id: index + 1,
      path: `C:/media/${index + 1}.jpg`
    }));
    const onVirtualRangeChange = vi.fn();

    render(
      <GalleryGrid
        assets={assets}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onVirtualRangeChange={onVirtualRangeChange}
        onSelect={() => {}}
      />
    );

    await waitFor(() => {
      expect(onVirtualRangeChange).toHaveBeenCalledWith(3, 9);
    });
  });

  it("requests next page when virtual range reaches list end", async () => {
    virtualItems = [
      { key: 0, index: 0, start: 0 },
      { key: 1, index: 39, start: 188 }
    ];
    const assets = Array.from({ length: 40 }, (_, index) => ({
      ...sampleAsset,
      id: index + 1,
      path: `C:/media/${index + 1}.jpg`
    }));
    const onReachEnd = vi.fn();

    render(
      <GalleryGrid
        assets={assets}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={onReachEnd}
        onSelect={() => {}}
      />
    );

    await waitFor(() => {
      expect(onReachEnd).toHaveBeenCalled();
    });
  });

  it("zooms tiles with ctrl + wheel over gallery", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];
    const onCtrlWheelZoom = vi.fn();

    const { container } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onCtrlWheelZoom={onCtrlWheelZoom}
        onSelect={() => {}}
      />
    );

    const gallery = container.querySelector("[data-testid='gallery-grid']") as HTMLElement;
    const wheelEvent = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      deltaY: -120
    });

    gallery.dispatchEvent(wheelEvent);

    expect(onCtrlWheelZoom).toHaveBeenCalledWith(-120);

    const plainWheelEvent = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      ctrlKey: false,
      deltaY: -120
    });
    gallery.dispatchEvent(plainWheelEvent);

    expect(onCtrlWheelZoom).toHaveBeenCalledTimes(1);
  });

  it("uses lightbox selection when bulk mode is disabled", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];
    const onSelect = vi.fn();
    const onBulkSelectionInteraction = vi.fn();

    const { container } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={onSelect}
        onBulkSelectionInteraction={onBulkSelectionInteraction}
      />
    );

    const tile = container.querySelector("button") as HTMLButtonElement;
    fireEvent.click(tile);

    expect(onSelect).toHaveBeenCalledWith(sampleAsset);
    expect(onBulkSelectionInteraction).not.toHaveBeenCalled();
  });

  it("reports ctrl and shift bulk interactions when bulk mode is enabled", () => {
    virtualItems = [{ key: 0, index: 0, start: 0 }];
    const onSelect = vi.fn();
    const onBulkSelectionInteraction = vi.fn();

    const { container } = render(
      <GalleryGrid
        assets={[sampleAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={onSelect}
        selectionModeEnabled
        selectedAssetIds={new Set()}
        onBulkSelectionInteraction={onBulkSelectionInteraction}
      />
    );

    const tile = container.querySelector("button") as HTMLButtonElement;
    fireEvent.click(tile, { ctrlKey: true });
    fireEvent.click(tile, { shiftKey: true });

    expect(onSelect).not.toHaveBeenCalled();
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(1, {
      assetId: sampleAsset.id,
      assetIndex: 0,
      ctrlLike: true,
      shift: false,
      viaDrag: false
    });
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(2, {
      assetId: sampleAsset.id,
      assetIndex: 0,
      ctrlLike: false,
      shift: true,
      viaDrag: false
    });
  });

  it("supports drag bulk selection in bulk mode", () => {
    const secondAsset = {
      ...sampleAsset,
      id: 9,
      path: "C:/media/b.jpg"
    };
    virtualItems = [
      { key: 0, index: 0, start: 0 },
      { key: 1, index: 1, start: 198 }
    ];
    const onBulkSelectionInteraction = vi.fn();

    const { container } = render(
      <GalleryGrid
        assets={[sampleAsset, secondAsset]}
        selectedId={null}
        thumbs={{}}
        tileSize={188}
        hasMore={false}
        isLoading={false}
        isGeneratingThumbnails={false}
        pendingThumbnailCount={0}
        renderingThumbnailIds={{}}
        onReachEnd={() => {}}
        onSelect={() => {}}
        selectionModeEnabled
        selectedAssetIds={new Set()}
        onBulkSelectionInteraction={onBulkSelectionInteraction}
      />
    );

    const tiles = container.querySelectorAll("button");
    fireEvent.mouseDown(tiles[0] as HTMLButtonElement);
    fireEvent.mouseEnter(tiles[1] as HTMLButtonElement);
    fireEvent.mouseUp(window);

    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(1, {
      assetId: sampleAsset.id,
      assetIndex: 0,
      ctrlLike: false,
      shift: false,
      viaDrag: true
    });
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(2, {
      assetId: secondAsset.id,
      assetIndex: 1,
      ctrlLike: false,
      shift: false,
      viaDrag: true
    });
  });
});
