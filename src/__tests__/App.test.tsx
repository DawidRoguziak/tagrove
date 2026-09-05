import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { MediaKind } from "../types";

type LegacyAssetRow = {
  id: number;
  path: string;
  kind: MediaKind;
  modified_at: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  thumb_path: string | null;
  is_favorite: boolean;
  media_group_key: string | null;
  media_group_order: number | null;
};

function createAsset(id: number, path: string) {
  return {
    id,
    path,
    kind: "image" as const,
    size_bytes: 1024,
    modified_at: 1700000000,
    width: 1200,
    height: 800,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: []
  };
}

function createTagListPage(items: string[] = [], total: number = items.length) {
  return { items, total };
}

function createAssetWithGroup(id: number, path: string, groupKey: string, groupOrder: number) {
  return {
    ...createAsset(id, path),
    media_group_key: groupKey,
    media_group_order: groupOrder
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res, _rej) => {
    resolve = res;
  });
  return { promise, resolve };
}

function selectAssetByPath(path: string) {
  const tile = screen.getByAltText(path).closest("button");
  if (!tile) throw new Error(`Missing gallery tile for ${path}`);
  fireEvent.mouseDown(tile, { button: 0 });
  fireEvent.mouseUp(window);
}

async function typeAssetTag(tag: string) {
  const input = screen.getByLabelText("Add tag");
  await userEvent.click(input);
  await userEvent.type(input, `${tag}{Enter}`);
}

const virtualizerState = vi.hoisted(() => ({ renderItems: false }));

const apiMocks = vi.hoisted(() => {
  return {
    addScanRoot: vi.fn(),
    cancelRenderAllThumbnails: vi.fn(),
    clearLibraryData: vi.fn(),
    exportDbBundle: vi.fn(),
    exportTagsCsv: vi.fn(),
    importDbBundle: vi.fn(),
    importTagsCsv: vi.fn(),
    startAssetQuery: vi.fn(),
    getAssetQueryPage: vi.fn(),
    getAssetDetails: vi.fn(),
    openVideo: vi.fn(async () => 1),
    setVideoBounds: vi.fn(async () => {}),
    controlVideo: vi.fn(async () => {}),
    closeVideo: vi.fn(async () => {}),
    ensureThumbnailsStream: vi.fn(),
    listAssets: vi.fn(),
    listScanRoots: vi.fn(),
    listTags: vi.fn(),
    mergeAssetTagsBulk: vi.fn(),
    setAssetsMediaGroupBulk: vi.fn(),
    removeScanRoot: vi.fn(),
    renderAllThumbnails: vi.fn(),
    renderFailedThumbnails: vi.fn(),
    scanFolder: vi.fn(),
    rescanAllRoots: vi.fn(),
    setAssetFavorite: vi.fn(),
    setAssetTags: vi.fn(),
    toMediaSrc: vi.fn((path: string) => `media://${path}`)
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn()
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => {
    return () => {};
  })
}));

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count, estimateSize, gap = 0, getItemKey }: {
    count: number;
    estimateSize: () => number;
    gap?: number;
    getItemKey: (index: number) => string | number | bigint;
  }) => {
    const size = estimateSize();
    const items = virtualizerState.renderItems ? Array.from({ length: count }, (_, index) => ({
      key: getItemKey(index),
      index,
      start: index * (size + gap),
      end: index * (size + gap) + size
    })) : [];
    return {
      getVirtualItems: () => items,
      getTotalSize: () => count * size + Math.max(0, count - 1) * gap,
      isScrolling: false
    };
  }
}));

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    ...apiMocks
  };
});

describe("App", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    virtualizerState.renderItems = false;
    window.localStorage.clear();
    apiMocks.listAssets.mockResolvedValue({ items: [], total: 0 });
    apiMocks.startAssetQuery.mockImplementation(async (params) => {
      const page = await apiMocks.listAssets({
        offset: 0,
        limit: params.pageSize,
        tagsAnd: params.tagsAnd,
        tagsNot: params.tagsNot,
        mediaKind: params.mediaKind,
        favoritesOnly: params.favoritesOnly,
        metaFilter: params.metaFilter ?? null
      });
      return {
        status: "ready",
        session_id: 1,
        revision: 1,
        total: page.total,
        offset: 0,
        items: page.items.map((asset: LegacyAssetRow) => ({
          id: asset.id,
          file_name: asset.path.split("/").at(-1) ?? asset.path,
          preview_path: asset.path,
          kind: asset.kind,
          modified_at: asset.modified_at,
          width: asset.width,
          height: asset.height,
          duration_ms: asset.duration_ms,
          thumb_path: asset.thumb_path,
          is_favorite: asset.is_favorite,
          media_group_key: asset.media_group_key,
          media_group_order: asset.media_group_order
        }))
      };
    });
    apiMocks.getAssetDetails.mockResolvedValue(null);
    apiMocks.ensureThumbnailsStream.mockResolvedValue(undefined);
    apiMocks.listTags.mockResolvedValue(createTagListPage());
    apiMocks.mergeAssetTagsBulk.mockImplementation(async (assetIds: number[], tags: string[]) => ({
      processed_assets: assetIds.length,
      updated_assets: assetIds.length,
      processed_asset_ids: assetIds,
      updated_asset_ids: assetIds,
      results: assetIds.map((assetId) => ({ asset_id: assetId, changed: true, tags })),
      revision: 2
    }));
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({ asset_id: assetId, changed: true, tags, revision: 2 }));
    apiMocks.setAssetsMediaGroupBulk.mockResolvedValue({
      processed_assets: 0,
      updated_assets: 0,
      media_group_key: null
    });
    apiMocks.listScanRoots.mockResolvedValue([]);
    apiMocks.cancelRenderAllThumbnails.mockResolvedValue(true);
    apiMocks.scanFolder.mockResolvedValue({ indexed: 0, removed: 0, failed: 0 });
    apiMocks.removeScanRoot.mockResolvedValue({ removed_assets: 0, removed_thumbnails: 0 });
    apiMocks.renderAllThumbnails.mockResolvedValue({
      generated: 0,
      failed: 0,
      skipped_failed: 0,
      processed: 0,
      total: 0,
      cancelled: false
    });
    apiMocks.renderFailedThumbnails.mockResolvedValue({
      generated: 0,
      failed: 0,
      skipped_failed: 0,
      processed: 0,
      total: 0,
      cancelled: false
    });
  });

  it("loads initial data and submits tag filters", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
      expect(apiMocks.listTags).toHaveBeenCalledWith({
        query: "",
        offset: 0,
        limit: 200
      });
      expect(apiMocks.listScanRoots).toHaveBeenCalled();
    });

    const input = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.clear(input);
    await userEvent.type(input, "cat -dog{Enter}");
    const mediaKindSelect = screen.getByRole("combobox", { name: "Media kind" }) as HTMLSelectElement;
    await userEvent.selectOptions(mediaKindSelect, "video");

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: ["cat"],
        tagsNot: ["dog"],
        mediaKind: "video",
        favoritesOnly: false,
        metaFilter: null
      });
    });
  });

  it("refreshes results right after changing media kind", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
    });

    const mediaKindSelect = screen.getByRole("combobox", { name: "Media kind" }) as HTMLSelectElement;
    await userEvent.selectOptions(mediaKindSelect, "video");

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: [],
        tagsNot: [],
        mediaKind: "video",
        favoritesOnly: false,
        metaFilter: null
      });
    });
  });

  it("clears search filters and resets media kind to all", async () => {
    render(<App />);

    const input = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.type(input, "cat{Enter}");
    const mediaKindSelect = screen.getByRole("combobox", { name: "Media kind" }) as HTMLSelectElement;
    await userEvent.selectOptions(mediaKindSelect, "video");

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: ["cat"],
        tagsNot: [],
        mediaKind: "video",
        favoritesOnly: false,
        metaFilter: null
      });
    });

    await userEvent.click(screen.getByRole("button", { name: "Clear all search filters" }));

    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe("");
      expect(mediaKindSelect.value).toBe("all");
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: [],
        tagsNot: [],
        mediaKind: "all",
        favoritesOnly: false,
        metaFilter: null
      });
    });
  });

  it("submits exact tag-count metatag filters", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
    });

    const input = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.clear(input);
    await userEvent.type(input, "tags:2{Enter}");

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: [],
        tagsNot: [],
        mediaKind: "all",
        favoritesOnly: false,
        metaFilter: {
          type: "hasNoTags",
          tagCount: 2
        }
      });
    });
  });

  it("submits exact group-name metatag filters", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
    });

    const input = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.clear(input);
    await userEvent.type(input, "gN:Trip-2026{Enter}");

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenLastCalledWith({
        offset: 0,
        limit: 128,
        tagsAnd: [],
        tagsNot: [],
        mediaKind: "all",
        favoritesOnly: false,
        metaFilter: {
          type: "groupName",
          groupName: "Trip-2026"
        }
      });
    });
  });

  it("shows validation errors for invalid metatag combinations", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
    });

    const callsBeforeSubmit = apiMocks.listAssets.mock.calls.length;
    const input = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.type(input, "cat tags{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Metatags must be used on their own, without other tags."
    );
    expect(apiMocks.listAssets.mock.calls.length).toBe(callsBeforeSubmit);
  });

  it("shows gallery empty state when no assets are returned", async () => {
    apiMocks.listScanRoots.mockResolvedValue(["C:/media"]);

    render(<App />);

    expect(await screen.findByRole("heading", { name: "No results" })).toBeInTheDocument();
    expect(
      screen.getByText("Try changing filters or run rescan to index more files.")
    ).toBeInTheDocument();
  });

  it("shows first-folder CTA and opens highlighted scan settings section", async () => {
    render(<App />);

    expect(await screen.findByRole("heading", { name: "No folders attached" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add first folder" }));

    expect(await screen.findByRole("button", { name: "Back" })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Choose folder" })).toBeInTheDocument();
    expect(screen.getByTestId("scan-settings-section")).toHaveAttribute("data-highlighted", "true");
  });

  it("shows end-of-list info when all assets are loaded", async () => {
    apiMocks.listAssets.mockResolvedValue({
      items: [
        createAsset(1, "C:/media/sample.jpg")
      ],
      total: 1
    });

    render(<App />);

    expect(await screen.findByText("No more items to load")).toBeInTheDocument();
  });

  it("supports bulk selection without exposing a select-all action", async () => {
    virtualizerState.renderItems = true;
    apiMocks.listAssets.mockResolvedValue({
      items: [
        createAsset(1, "C:/media/a.jpg"),
        createAsset(2, "C:/media/b.jpg"),
        createAsset(3, "C:/media/c.jpg")
      ],
      total: 3
    });

    render(<App />);

    await screen.findByText("No more items to load");

    const enableSelectionButton = screen.getByRole("button", { name: "Enable bulk actions" });
    await userEvent.click(enableSelectionButton);

    const panel = await screen.findByTestId("bulk-action-panel");
    expect(panel).toBeInTheDocument();
    expect(panel.parentElement).toHaveClass("workspace-body", "workspace-body--bulk");
    expect(panel.closest("main")).not.toHaveClass("grid-rows-[auto_1fr]");
    expect(screen.getByText("Selected: 0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Select all" })).not.toBeInTheDocument();

    selectAssetByPath("C:/media/a.jpg");
    selectAssetByPath("C:/media/b.jpg");
    selectAssetByPath("C:/media/c.jpg");
    expect(screen.getByText("Selected: 3")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Disable bulk actions" }));
    expect(screen.queryByTestId("bulk-action-panel")).not.toBeInTheDocument();
  });

  it("applies bulk tag merge for selected assets", async () => {
    virtualizerState.renderItems = true;
    apiMocks.listAssets.mockResolvedValue({
      items: [
        createAsset(1, "C:/media/a.jpg"),
        createAsset(2, "C:/media/b.jpg")
      ],
      total: 2
    });
    apiMocks.listTags
      .mockResolvedValueOnce(createTagListPage())
      .mockResolvedValue(createTagListPage(["cat", "travel"]));

    render(<App />);

    await screen.findByText("No more items to load");
    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");
    selectAssetByPath("C:/media/b.jpg");

    const input = screen.getByLabelText("Add tag");
    const listTagsCallsBeforeApply = apiMocks.listTags.mock.calls.length;
    await userEvent.type(input, "travel{Enter}");

    await waitFor(() => {
      expect(apiMocks.mergeAssetTagsBulk).toHaveBeenCalledWith([1, 2], ["travel"]);
    });
    expect(await screen.findByText("travel")).toBeInTheDocument();
    await waitFor(() => {
      expect(apiMocks.listTags.mock.calls.length).toBeGreaterThan(listTagsCallsBeforeApply);
    });
  });

  it("loads, removes, and automatically adds tags for one selected asset", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockResolvedValue({ ...asset, tags: ["cat", "dog"] });

    render(<App />);

    await screen.findByText("No more items to load");
    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");

    expect(await screen.findByText("cat")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove tag cat" }));
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["dog"]);
    });

    await typeAssetTag("travel");
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["dog", "travel"]);
    });
  });

  it("preserves canonical tags across lightbox to bulk to lightbox editing", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockResolvedValue({ ...asset, tags: ["cat"] });
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({
      asset_id: assetId, changed: true, tags, revision: 2
    }));

    render(<App />);
    await screen.findByText("No more items to load");
    const tile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!tile) throw new Error("Missing gallery tile");
    await userEvent.click(tile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledWith(1));
    await userEvent.click(await screen.findByRole("button", { name: "Close preview" }));

    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");
    const bulkInput = await screen.findByLabelText("Add tag");
    await userEvent.type(bulkInput, "dog{Enter}");
    await waitFor(() => expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["cat", "dog"]));
    await userEvent.click(screen.getByRole("button", { name: "Disable bulk actions" }));

    await userEvent.click(tile);
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    expect(screen.getByText("dog")).toBeInTheDocument();
    const birdInput = screen.getByLabelText("Add tag");
    await userEvent.click(birdInput);
    await userEvent.type(birdInput, "bird{Enter}");
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "dog", "bird"]);
    });
  });

  it("prevents bulk IPC while a lightbox tag write owns the shared asset lock", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    const pendingLightboxSave = deferred<{
      asset_id: number; changed: boolean; tags: string[]; revision: number;
    }>();
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockResolvedValue({ ...asset, tags: ["cat"] });
    apiMocks.setAssetTags.mockClear();
    apiMocks.setAssetTags
      .mockReturnValueOnce(pendingLightboxSave.promise)
      .mockImplementation(async (assetId: number, tags: string[]) => ({
        asset_id: assetId, changed: true, tags, revision: 3
      }));

    render(<App />);
    await screen.findByText("No more items to load");
    const tile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!tile) throw new Error("Missing gallery tile");
    await userEvent.click(tile);
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    await typeAssetTag("dog");
    await waitFor(() => expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(1));
    await userEvent.click(screen.getByRole("button", { name: "Close preview" }));

    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");
    const bulkInput = await screen.findByLabelText("Add tag");
    await userEvent.type(bulkInput, "bird{Enter}");
    expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(1);
    expect(bulkInput).toHaveValue("bird");

    await act(async () => {
      pendingLightboxSave.resolve({
        asset_id: 1, changed: true, tags: ["cat", "dog"], revision: 2
      });
      await pendingLightboxSave.promise;
    });
    await waitFor(() => expect(screen.getByText("dog")).toBeInTheDocument());
    await userEvent.type(bulkInput, "{Enter}");
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(2);
      expect(apiMocks.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "dog", "bird"]);
    });
  });

  it("prevents lightbox IPC while a bulk tag write owns the shared asset lock", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    const pendingBulkSave = deferred<{
      asset_id: number; changed: boolean; tags: string[]; revision: number;
    }>();
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockResolvedValue({ ...asset, tags: ["cat"] });
    apiMocks.setAssetTags.mockClear();
    apiMocks.setAssetTags
      .mockReturnValueOnce(pendingBulkSave.promise)
      .mockImplementation(async (assetId: number, tags: string[]) => ({
        asset_id: assetId, changed: true, tags, revision: 3
      }));

    render(<App />);
    await screen.findByText("No more items to load");
    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");
    const bulkInput = await screen.findByLabelText("Add tag");
    await userEvent.type(bulkInput, "dog{Enter}");
    await waitFor(() => expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(1));
    await userEvent.click(screen.getByRole("button", { name: "Disable bulk actions" }));

    const tile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!tile) throw new Error("Missing gallery tile");
    await userEvent.click(tile);
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    await typeAssetTag("bird");
    await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument());
    expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(1);

    await act(async () => {
      pendingBulkSave.resolve({
        asset_id: 1, changed: true, tags: ["cat", "dog"], revision: 2
      });
      await pendingBulkSave.promise;
    });
    await waitFor(() => expect(screen.getByText("dog")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(2);
      expect(apiMocks.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "dog", "bird"]);
    });
  });

  it("invalidates cached details after successful CSV import and preserves imported tags on edit", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockClear();
    apiMocks.getAssetDetails
      .mockResolvedValueOnce({ ...asset, tags: ["old"] })
      .mockResolvedValueOnce({ ...asset, tags: ["old", "local", "imported"] });
    const pendingSave = deferred<{
      asset_id: number; changed: boolean; tags: string[]; revision: number;
    }>();
    apiMocks.importTagsCsv.mockClear();
    apiMocks.importTagsCsv.mockResolvedValueOnce({
      rows_read: 1, rows_applied: 1, assets_matched: 1, assets_updated: 1
    });
    apiMocks.setAssetTags.mockClear();
    apiMocks.setAssetTags.mockReturnValueOnce(pendingSave.promise);
    vi.mocked(open).mockReset().mockResolvedValueOnce("C:/tmp/tags.csv");

    render(<App />);
    await screen.findByText("No more items to load");
    const tile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!tile) throw new Error("Missing gallery tile");
    await userEvent.click(tile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    await typeAssetTag("local");
    await waitFor(() => expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["old", "local"]));
    await userEvent.click(await screen.findByRole("button", { name: "Close preview" }));

    await userEvent.click(screen.getByRole("button", { name: "Open settings" }));
    await userEvent.click(await screen.findByRole("button", { name: "Import tags CSV" }));
    await Promise.resolve();
    expect(apiMocks.importTagsCsv).not.toHaveBeenCalled();
    pendingSave.resolve({ asset_id: 1, changed: true, tags: ["old", "local"], revision: 2 });
    await waitFor(() => expect(apiMocks.importTagsCsv).toHaveBeenCalledWith("C:/tmp/tags.csv"));
    await waitFor(() => expect(screen.getByText(/CSV imported\. Rows read: 1/i)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Back" }));

    const reopenedTile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!reopenedTile) throw new Error("Missing reopened gallery tile");
    await userEvent.click(reopenedTile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    expect(screen.getByText("imported")).toBeInTheDocument();
    await typeAssetTag("new");
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenLastCalledWith(1, ["old", "local", "imported", "new"]);
    });
  });

  it("invalidates cached details after rejected CSV import with partial backend changes", async () => {
    virtualizerState.renderItems = true;
    const asset = createAsset(1, "C:/media/a.jpg");
    apiMocks.listAssets.mockResolvedValue({ items: [asset], total: 1 });
    apiMocks.getAssetDetails.mockClear();
    apiMocks.getAssetDetails
      .mockResolvedValueOnce({ ...asset, tags: ["old"] })
      .mockResolvedValueOnce({ ...asset, tags: ["old", "local", "partially-imported"] });
    const pendingSave = deferred<{
      asset_id: number; changed: boolean; tags: string[]; revision: number;
    }>();
    apiMocks.importTagsCsv.mockClear();
    apiMocks.importTagsCsv.mockRejectedValueOnce(new Error("malformed later row"));
    apiMocks.setAssetTags.mockClear();
    apiMocks.setAssetTags.mockReturnValueOnce(pendingSave.promise);
    vi.mocked(open).mockReset().mockResolvedValueOnce("C:/tmp/partial.csv");

    render(<App />);
    await screen.findByText("No more items to load");
    const tile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!tile) throw new Error("Missing gallery tile");
    await userEvent.click(tile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    await typeAssetTag("local");
    await waitFor(() => expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["old", "local"]));
    await userEvent.click(await screen.findByRole("button", { name: "Close preview" }));

    await userEvent.click(screen.getByRole("button", { name: "Open settings" }));
    await userEvent.click(await screen.findByRole("button", { name: "Import tags CSV" }));
    await Promise.resolve();
    expect(apiMocks.importTagsCsv).not.toHaveBeenCalled();
    pendingSave.resolve({ asset_id: 1, changed: true, tags: ["old", "local"], revision: 2 });
    await waitFor(() => expect(apiMocks.importTagsCsv).toHaveBeenCalledWith("C:/tmp/partial.csv"));
    await waitFor(() => expect(screen.getByText(/malformed later row/i)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Back" }));

    const reopenedTile = screen.getByAltText("C:/media/a.jpg").closest("button");
    if (!reopenedTile) throw new Error("Missing reopened gallery tile");
    await userEvent.click(reopenedTile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    expect(screen.getByText("partially-imported")).toBeInTheDocument();
    await typeAssetTag("new");
    await waitFor(() => {
      expect(apiMocks.setAssetTags).toHaveBeenLastCalledWith(1, ["old", "local", "partially-imported", "new"]);
    });
    apiMocks.importTagsCsv.mockClear();
  });

  it("resets shell tag identity so a cleared-library ID is loaded fresh", async () => {
    virtualizerState.renderItems = true;
    const oldAsset = createAsset(1, "C:/media/old.jpg");
    const reusedAsset = createAsset(1, "C:/media/reused.jpg");
    apiMocks.listAssets.mockResolvedValue({ items: [oldAsset], total: 1 });
    apiMocks.getAssetDetails.mockClear();
    apiMocks.getAssetDetails.mockResolvedValueOnce({ ...oldAsset, tags: ["old"] });
    apiMocks.clearLibraryData.mockClear();
    apiMocks.clearLibraryData.mockResolvedValue({
      removed_assets: 1, removed_roots: 0, removed_thumbnails: 0
    });

    render(<App />);
    await screen.findByText("No more items to load");
    const oldTile = screen.getByAltText("C:/media/old.jpg").closest("button");
    if (!oldTile) throw new Error("Missing old gallery tile");
    await userEvent.click(oldTile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1));
    await userEvent.click(await screen.findByRole("button", { name: "Close preview" }));

    await userEvent.click(screen.getByRole("button", { name: "Open settings" }));
    await userEvent.click(await screen.findByRole("button", { name: "Remove all thumbnails, indexed assets and scan paths" }));
    await userEvent.click(screen.getByRole("button", { name: "YES" }));
    await waitFor(() => expect(apiMocks.clearLibraryData).toHaveBeenCalledTimes(1));

    apiMocks.listAssets.mockResolvedValue({ items: [reusedAsset], total: 1 });
    apiMocks.getAssetDetails.mockResolvedValueOnce({ ...reusedAsset, tags: ["fresh"] });
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    const searchInput = screen.getByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026");
    await userEvent.type(searchInput, "{Enter}");
    const reusedTile = (await screen.findByAltText("C:/media/reused.jpg")).closest("button");
    if (!reusedTile) throw new Error("Missing reused gallery tile");
    await userEvent.click(reusedTile);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByLabelText("Add tag")).not.toBeDisabled());
    expect(screen.getByText("fresh")).toBeInTheDocument();
  });

  it("applies bulk media group with reordered selected assets", async () => {
    virtualizerState.renderItems = true;
    apiMocks.listAssets.mockResolvedValue({
      items: [
        createAssetWithGroup(1, "C:/media/a.jpg", "legacy", 90),
        createAsset(2, "C:/media/b.jpg")
      ],
      total: 2
    });

    render(<App />);

    await screen.findByText("No more items to load");
    await userEvent.click(screen.getByRole("button", { name: "Enable bulk actions" }));
    selectAssetByPath("C:/media/a.jpg");
    selectAssetByPath("C:/media/b.jpg");
    expect(screen.getByTestId("bulk-action-panel")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Multiple group assignments");

    await userEvent.type(screen.getByLabelText("Media group key"), "trip-2026");

    const firstHandle = screen.getByTestId("bulk-group-drag-handle-1");
    const secondTile = screen.getByTestId("bulk-group-tile-2");
    fireEvent.pointerDown(firstHandle, { button: 0 });
    fireEvent.pointerEnter(secondTile);
    fireEvent.pointerUp(window);

    await userEvent.click(screen.getByRole("button", { name: "Apply group" }));

    await waitFor(() => {
      expect(apiMocks.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
        [
          { assetId: 2, mediaGroupOrder: 1 },
          { assetId: 1, mediaGroupOrder: 2 }
        ],
        "trip-2026"
      );
    });

    await userEvent.clear(screen.getByLabelText("Media group key"));
    await userEvent.click(screen.getByRole("button", { name: "Remove from groups" }));
    await waitFor(() => {
      expect(apiMocks.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
        [
          { assetId: 2, mediaGroupOrder: null },
          { assetId: 1, mediaGroupOrder: null }
        ],
        null
      );
    });
  });

  it("opens settings as a standalone view and returns to gallery", async () => {
    render(<App />);

    expect(await screen.findByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026")).toBeInTheDocument();

    const settingsButton = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsButton);

    expect(await screen.findByRole("button", { name: "Back" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clear all search filters" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Back" }));

    expect(await screen.findByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Open settings" })).toHaveFocus();
    });
  });

  it("closes settings with Escape key", async () => {
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Open settings" }));
    expect(await screen.findByRole("button", { name: "Back" })).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");

    expect(await screen.findByPlaceholderText("Tags: cat vacation -dog | tags | tags:3 | gN:Trip 2026")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
  });

  it("waits for in-app confirmation before removing scan path", async () => {
    apiMocks.listScanRoots.mockResolvedValue(["C:/media"]);
    apiMocks.removeScanRoot.mockResolvedValueOnce({ removed_assets: 2, removed_thumbnails: 3 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Open settings" }));

    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));

    const cancelDialog = await screen.findByRole("dialog");
    expect(apiMocks.removeScanRoot).not.toHaveBeenCalled();
    await userEvent.click(within(cancelDialog).getByRole("button", { name: "Cancel" }));
    expect(apiMocks.removeScanRoot).not.toHaveBeenCalled();

    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));

    const confirmDialog = await screen.findByRole("dialog");
    await userEvent.click(within(confirmDialog).getByRole("button", { name: "Remove" }));

    await waitFor(() => {
      expect(apiMocks.removeScanRoot).toHaveBeenCalledWith("C:/media");
    });
  });

  it("allows stopping active thumbnail bulk render and retrying failed thumbnails", async () => {
    const pending = deferred<{
      generated: number;
      failed: number;
      skipped_failed: number;
      processed: number;
      total: number;
      cancelled: boolean;
    }>();
    apiMocks.renderAllThumbnails.mockReturnValueOnce(pending.promise);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);

    await userEvent.click(
      await screen.findByRole("button", { name: "Render thumbnails for all indexed assets" })
    );

    const stopButton = await screen.findByRole("button", { name: "Stop thumbnail render" });
    await userEvent.click(stopButton);
    expect(apiMocks.cancelRenderAllThumbnails).toHaveBeenCalledTimes(1);

    pending.resolve({
      generated: 3,
      failed: 1,
      skipped_failed: 2,
      processed: 8,
      total: 12,
      cancelled: true
    });

    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "Stop thumbnail render" })).not.toBeInTheDocument();
    });

    await userEvent.click(await screen.findByRole("button", { name: "Retry failed thumbnails only" }));
    await waitFor(() => {
      expect(apiMocks.renderFailedThumbnails).toHaveBeenCalledTimes(1);
    });
  });

  it("shows section loader for scan actions", async () => {
    const pending = deferred<{
      generated: number;
      failed: number;
      skipped_failed: number;
      processed: number;
      total: number;
      cancelled: boolean;
    }>();
    apiMocks.renderAllThumbnails.mockReturnValueOnce(pending.promise);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);
    await userEvent.click(
      await screen.findByRole("button", { name: "Render thumbnails for all indexed assets" })
    );

    expect(await screen.findByTestId("scan-section-loader")).toBeInTheDocument();

    pending.resolve({
      generated: 1,
      failed: 0,
      skipped_failed: 0,
      processed: 1,
      total: 1,
      cancelled: false
    });

    await waitFor(() => {
      expect(screen.queryByTestId("scan-section-loader")).not.toBeInTheDocument();
    });
  });

  it("shows section loader for import export actions", async () => {
    const pending = deferred<{ rows: number }>();
    vi.mocked(save).mockResolvedValueOnce("C:/tmp/tags-export.csv");
    apiMocks.exportTagsCsv.mockReturnValueOnce(pending.promise);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);
    await userEvent.click(await screen.findByRole("button", { name: "Export tags CSV" }));

    expect(await screen.findByTestId("import-export-section-loader")).toBeInTheDocument();

    pending.resolve({ rows: 10 });

    await waitFor(() => {
      expect(screen.queryByTestId("import-export-section-loader")).not.toBeInTheDocument();
    });
  });

  it("uses timestamped default file name for CSV export", async () => {
    vi.mocked(save).mockResolvedValueOnce(null);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);
    await userEvent.click(await screen.findByRole("button", { name: "Export tags CSV" }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          title: "Export tags CSV",
          defaultPath: expect.stringMatching(/^tags-export-\d{4}-\d{2}-\d{2}_\d{2}-\d{2}\.csv$/),
          filters: [{ name: "CSV", extensions: ["csv"] }]
        })
      );
    });
  });

  it("uses timestamped default file name for DB bundle export", async () => {
    vi.mocked(save).mockResolvedValueOnce(null);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);
    await userEvent.click(await screen.findByRole("button", { name: "Export DB + thumbnails" }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          title: "Export DB backup archive",
          defaultPath: expect.stringMatching(/^media-backup-\d{4}-\d{2}-\d{2}_\d{2}-\d{2}\.zip$/),
          filters: [{ name: "ZIP", extensions: ["zip"] }]
        })
      );
    });
  });

  it("blocks other settings actions while async scan operation is running", async () => {
    const pending = deferred<{
      generated: number;
      failed: number;
      skipped_failed: number;
      processed: number;
      total: number;
      cancelled: boolean;
    }>();
    apiMocks.renderAllThumbnails.mockReturnValueOnce(pending.promise);

    render(<App />);

    const settingsToggle = await screen.findByRole("button", { name: "Open settings" });
    await userEvent.click(settingsToggle);

    await userEvent.click(
      await screen.findByRole("button", { name: "Render thumbnails for all indexed assets" })
    );

    const chooseFolderButton = await screen.findByRole("button", { name: "Choose folder" });
    const importCsvButton = await screen.findByRole("button", { name: "Import tags CSV" });

    expect(chooseFolderButton).toBeDisabled();
    expect(importCsvButton).toBeDisabled();

    await userEvent.click(importCsvButton);
    expect(apiMocks.importTagsCsv).not.toHaveBeenCalled();

    pending.resolve({
      generated: 1,
      failed: 0,
      skipped_failed: 0,
      processed: 1,
      total: 1,
      cancelled: false
    });

    await waitFor(() => {
      expect(screen.queryByTestId("scan-section-loader")).not.toBeInTheDocument();
    });
  });

  it("shows theme select only in settings and applies selected theme", async () => {
    render(<App />);

    await waitFor(() => {
      expect(apiMocks.listAssets).toHaveBeenCalled();
    });

    expect(screen.queryByRole("combobox", { name: "Select theme" })).not.toBeInTheDocument();

    await userEvent.click(await screen.findByRole("button", { name: "Open settings" }));

    const themeSelect = await screen.findByRole("combobox", { name: "Select theme" });
    await userEvent.selectOptions(themeSelect, "light");

    await waitFor(() => {
      expect(document.documentElement.getAttribute("data-theme")).toBe("light");
      expect(window.localStorage.getItem("media-tagger.theme")).toBe("light");
    });
  });
});





