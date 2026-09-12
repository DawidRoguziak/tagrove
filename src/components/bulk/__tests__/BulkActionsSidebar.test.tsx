import { UiLayerProvider } from "../../UI/UiLayerProvider";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../types";
import type { BulkSelectionController } from "../../app/types";
import { BulkActionsSidebar } from "../BulkActionsSidebar";

vi.mock("@tanstack/react-virtual", async () => {
  const actual = await vi.importActual<typeof import("@tanstack/react-virtual")>("@tanstack/react-virtual");
  return { ...actual, useVirtualizer: (options: Parameters<typeof actual.useVirtualizer>[0]) => actual.useVirtualizer({
    ...options, observeElementRect: (_instance, callback) => { callback({ width: 320, height: 240 }); return () => {}; }
  }) };
});

function createAsset(id: number, override: Partial<AssetSummary> = {}): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    ...override
  };
}

function createController(
  override: Partial<BulkSelectionController> = {}
): BulkSelectionController {
  return {
    favoriteApplying: false,
    favoriteFailed: false,
    allSelectedFavorites: false,
    onToggleFavorite: vi.fn(async () => {}),
    selectionModeEnabled: true,
    selectedAssetIds: new Set(),
    selectedAssets: [],
    startupPopularTags: [],
    knownTags: ["cat", "travel"],
    groupKeyDraft: "",
    orderedAssetIds: [],
    hasConflictingGroups: false,
    groupApplying: false,
    groupFailed: false,
    tagMode: "none",
    singleAssetTags: [],
    appliedBulkTags: [],
    tagDetailsLoading: false,
    tagDetailsFailed: false,
    tagApplying: false,
    tagSaveFailed: false,
    onToggleSelectionMode: vi.fn(),
    onBulkSelectionInteraction: vi.fn(),
    onGroupKeyDraftChange: vi.fn(),
    onReorderGroupAsset: vi.fn(),
    onApplyGroup: vi.fn<BulkSelectionController["onApplyGroup"]>(async () => ({ status: "saved" })),
    onAddTag: vi.fn(async () => true),
    onRemoveTag: vi.fn(async () => {}),
    onRetryTagDetails: vi.fn(),
    ...override
  };
}

describe("BulkActionsSidebar", () => {
  afterEach(cleanup);

  it("shows startup tags in snapshot order and keeps assigned single-asset tags clickable", async () => {
    const controller = createController({ selectedAssetIds: new Set([1]), tagMode: "single",
      singleAssetTags: ["travel"], startupPopularTags: ["travel", "cat"] });
    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    const group = screen.getByRole("group", { name: "Most used tags" });
    expect(Array.from(group.querySelectorAll("button"), (button) => button.textContent)).toEqual(["travel", "cat"]);
    await userEvent.click(screen.getByRole("button", { name: "travel" }));
    expect(controller.onAddTag).toHaveBeenCalledWith("travel");
    await waitFor(() => expect(screen.getByLabelText("Add tag")).toHaveFocus());
    expect(screen.getByLabelText("Add tag")).toHaveValue("");
    expect(screen.getByRole("button", { name: "travel" })).toBeVisible();
  });

  it.each(["{Enter}", " "])("activates popular tags with keyboard %s", async (key) => {
    const controller = createController({ selectedAssetIds: new Set([1, 2]), tagMode: "multiple",
      startupPopularTags: ["travel"] });
    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    screen.getByRole("button", { name: "travel" }).focus();
    await userEvent.keyboard(key);
    expect(controller.onAddTag).toHaveBeenCalledExactlyOnceWith("travel");
    await waitFor(() => expect(screen.getByLabelText("Add tag")).toHaveFocus());
  });

  it.each<Partial<BulkSelectionController>>([
    { selectedAssetIds: new Set() }, { selectionBusy: true }, { tagApplying: true },
    { tagDetailsLoading: true }, { tagDetailsFailed: true }
  ])("disables popular tags under the input's disabled conditions: %j", async (state) => {
    const controller = createController({ selectedAssetIds: new Set([1]), startupPopularTags: ["travel"], ...state });
    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    const chip = screen.getByRole("button", { name: "travel" });
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(chip).toBeDisabled();
    await userEvent.click(chip);
    expect(controller.onAddTag).not.toHaveBeenCalled();
  });

  it("retains a failed chip submission as the draft for ordinary Enter retry", async () => {
    const onAddTag = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const controller = createController({ selectedAssetIds: new Set([1, 2]), tagMode: "multiple",
      startupPopularTags: ["travel"], onAddTag, tagSaveFailed: true });
    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    await userEvent.click(screen.getByRole("button", { name: "travel" }));
    const input = screen.getByLabelText("Add tag");
    await waitFor(() => expect(input).toHaveFocus());
    expect(input).toHaveValue("travel");
    expect(screen.getByRole("alert")).toBeVisible();
    await userEvent.keyboard("{Enter}");
    expect(onAddTag).toHaveBeenNthCalledWith(2, "travel");
    await waitFor(() => expect(input).toHaveValue(""));
  });

  it("hides the popular tag section for an empty snapshot", () => {
    render(<BulkActionsSidebar controller={createController()} thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.queryByRole("group", { name: "Most used tags" })).not.toBeInTheDocument();
  });

  it("renders the lightbox-style heart and exposes pending and failure states", async () => {
    const onToggleFavorite = vi.fn(async () => {});
    const { rerender } = render(<BulkActionsSidebar controller={createController()}
      thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.getByRole("button", { name: "Toggle favorites for selected items" })).toBeDisabled();
    const controller = createController({ selectedAssetIds: new Set([1]), onToggleFavorite });
    rerender(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    const heart = screen.getByRole("button", { name: "Toggle favorites for selected items" });
    expect(heart).toBeEnabled(); // Selected IDs need not have loaded gallery rows.
    await userEvent.click(heart);
    expect(onToggleFavorite).toHaveBeenCalledOnce();
    rerender(<BulkActionsSidebar controller={{ ...controller, allSelectedFavorites: true, favoriteApplying: true }}
      thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.getByRole("button", { name: "Remove selected items from favorites" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove selected items from favorites" })).toHaveAttribute("aria-pressed", "true");
    rerender(<BulkActionsSidebar controller={{ ...controller, favoriteFailed: true }}
      thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.getByText("Could not update favorites. Try again.")).toBeVisible();
  });

  it("disables every write control while rectangle IDs are resolving", () => {
    const controller = createController({ selectionBusy: true, selectedAssetIds: new Set([1]),
      selectedAssets: [createAsset(1)], tagMode: "single", singleAssetTags: ["cat"] });
    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.getByRole("button", { name: "Toggle favorites for selected items" })).toBeDisabled();
    expect(screen.getByLabelText("Media group key")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove from group" })).toBeDisabled();
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove tag cat" })).toBeDisabled();
  });

  it("renders as a scrollable inspector with disabled editors for an empty selection", () => {
    render(
      <BulkActionsSidebar controller={createController()} thumbs={{}} renderingThumbnailIds={{}} />
    );

    const sidebar = screen.getByTestId("bulk-action-panel");
    expect(sidebar).toHaveClass("bulk-inspector", "panel-scroll");
    for (const panelId of ["bulk-header-panel", "bulk-group-panel", "bulk-tags-panel"]) {
      expect(sidebar).toContainElement(screen.getByTestId(panelId));
    }
    expect(screen.getByText("Selected: 0")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Clear selection" })).not.toBeInTheDocument();
    expect(screen.queryByText("Esc clears selection")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Select all" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Media group key")).toBeDisabled();
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.queryByTestId("bulk-group-order-list")).not.toBeInTheDocument();
    expect(screen.getByTestId("bulk-tags-panel")).toHaveClass("min-h-0", "overflow-visible");
    expect(screen.getByTestId("bulk-tag-list")).toHaveClass(
      "panel-scroll",
      "max-h-36",
      "overflow-y-auto",
      "p-2"
    );
    expect(screen.getByTestId("bulk-tag-list")).not.toHaveClass("p-1", "w-fit");
    expect(screen.getByTestId("bulk-tag-list")).toHaveAttribute("data-ui", "assigned-tag-list");
    expect(screen.getByText("Select at least one item")).toBeInTheDocument();
  });

  it("shows conflicts and reorders the compact list from its drag handle", () => {
    const first = createAsset(1, { media_group_key: "one", media_group_order: 1 });
    const second = createAsset(2, { media_group_key: "two", media_group_order: 1 });
    const onReorderGroupAsset = vi.fn();
    const controller = createController({
      selectedAssetIds: new Set([1, 2]),
      selectedAssets: [first, second],
      orderedAssetIds: [1, 2],
      groupKeyDraft: "replacement-group",
      hasConflictingGroups: true,
      tagMode: "multiple",
      onReorderGroupAsset
    });

    render(<BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} />);

    expect(screen.getByTestId("bulk-action-panel")).toHaveClass(
      "bulk-inspector"
    );
    expect(screen.getByTestId("bulk-group-order-panel")).toHaveClass(
      "max-h-[300px]",
      "min-h-0",
      "overflow-hidden"
    );
    expect(screen.getByTestId("bulk-group-order-list")).toHaveClass(
      "panel-scroll",
      "min-h-0",
      "overflow-y-auto"
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Multiple group assignments");
    expect(screen.getByText("1.jpg")).toBeInTheDocument();
    expect(screen.getByText("2.jpg")).toBeInTheDocument();
    expect(screen.getByTestId("bulk-group-thumbnail-1")).toHaveClass("h-16", "w-20");
    const firstTile = screen.getByTestId("bulk-group-tile-1");
    const firstHandle = screen.getByTestId("bulk-group-drag-handle-1");
    expect(firstTile).not.toHaveAttribute("draggable");
    expect(firstHandle).not.toHaveAttribute("draggable");
    expect(firstHandle).toHaveAttribute("aria-label", "Reorder item 1 with drag or arrow keys");
    expect(firstHandle).toHaveRole("button");

    fireEvent.pointerDown(firstHandle, { button: 0 });
    expect(firstTile).toHaveClass("opacity-60", "ring-2");
    fireEvent.pointerEnter(screen.getByTestId("bulk-group-tile-2"));
    expect(onReorderGroupAsset).toHaveBeenCalledWith(1, 2);
    fireEvent.pointerEnter(screen.getByTestId("bulk-group-tile-2"));
    expect(onReorderGroupAsset).toHaveBeenCalledTimes(1);
    fireEvent.pointerUp(window);
    expect(firstTile).not.toHaveClass("opacity-60", "ring-2");

    fireEvent.keyDown(firstHandle, { key: "ArrowDown" });
    expect(onReorderGroupAsset).toHaveBeenLastCalledWith(1, 2);
    fireEvent.keyDown(firstHandle, { key: "ArrowUp" });
    expect(onReorderGroupAsset).toHaveBeenCalledTimes(2);
  });

  it("does not expose group ordering until a group key is set", () => {
    const assets = [createAsset(1), createAsset(2)];
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([1, 2]),
          selectedAssets: assets,
          orderedAssetIds: [1, 2],
          groupKeyDraft: "",
          tagMode: "multiple"
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    expect(screen.queryByTestId("bulk-group-order-panel")).not.toBeInTheDocument();
    expect(screen.queryByTestId("bulk-group-drag-handle-1")).not.toBeInTheDocument();
  });

  it("removes and automatically adds tags for a single selected asset", async () => {
    const asset = createAsset(7);
    const onAddTag = vi.fn(async () => true);
    const onRemoveTag = vi.fn(async () => {});
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([7]),
          selectedAssets: [asset],
          orderedAssetIds: [7],
          tagMode: "single",
          singleAssetTags: ["cat"],
          onAddTag,
          onRemoveTag
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    expect(screen.getByTestId("bulk-action-panel")).toHaveClass(
      "bulk-inspector"
    );
    expect(screen.queryByTestId("bulk-group-order-panel")).not.toBeInTheDocument();
    expect(screen.getByTestId("bulk-tags-panel")).toHaveClass("min-h-0", "overflow-visible");
    await userEvent.click(screen.getByRole("button", { name: "Remove tag cat" }));
    expect(onRemoveTag).toHaveBeenCalledWith("cat");

    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "tra");
    const suggestions = await screen.findByRole("listbox", {
      name: "Bulk tagging suggestions"
    });
    expect(suggestions).toHaveClass("fixed");
    expect(screen.getByTestId("bulk-action-panel")).not.toContainElement(suggestions);
    expect(suggestions).not.toHaveClass("bottom-[calc(100%+8px)]");
    await userEvent.clear(input);
    await userEvent.type(input, "travel{Enter}");
    expect(onAddTag).toHaveBeenCalledWith("travel");
  });

  it("retains the selected suggestion, not the search prefix, when saving fails", async () => {
    const asset = createAsset(1);
    const onAddTag = vi.fn(async () => false);
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([1]),
          selectedAssets: [asset],
          orderedAssetIds: [1],
          tagMode: "single",
          onAddTag
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "ca");
    await userEvent.click((await screen.findAllByRole("option"))[0]!);

    await waitFor(() => {
      expect(onAddTag).toHaveBeenCalledWith("cat");
      expect(input).toHaveValue("cat");
    });

    await userEvent.type(input, "{Enter}");
    await waitFor(() => expect(onAddTag).toHaveBeenNthCalledWith(2, "cat"));
  });

  it("restores tag input focus after an Enter submission so another tag can be added", async () => {
    const asset = createAsset(1);
    let resolveFirstAdd: ((saved: boolean) => void) | undefined;
    const onAddTag = vi
      .fn<(tag: string) => Promise<boolean>>()
      .mockImplementationOnce(
        () =>
          new Promise<boolean>((resolve) => {
            resolveFirstAdd = resolve;
          })
      )
      .mockResolvedValue(true);

    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([1]),
          selectedAssets: [asset],
          orderedAssetIds: [1],
          tagMode: "single",
          onAddTag
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "travel{Enter}");
    await userEvent.click(screen.getByLabelText("Media group key"));
    expect(input).not.toHaveFocus();

    await act(async () => {
      resolveFirstAdd?.(true);
    });

    await waitFor(() => {
      expect(input).toHaveFocus();
      expect(input).toHaveValue("");
    });

    await userEvent.type(input, "summer{Enter}");
    await waitFor(() => {
      expect(onAddTag).toHaveBeenNthCalledWith(2, "summer");
      expect(input).toHaveFocus();
    });
  });

  it("shows applied tags without removal controls in multi-selection mode", () => {
    const assets = [createAsset(1), createAsset(2)];
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([1, 2]),
          selectedAssets: assets,
          orderedAssetIds: [1, 2],
          tagMode: "multiple",
          appliedBulkTags: ["travel"]
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    expect(screen.getByText("travel")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove tag travel" })).not.toBeInTheDocument();
  });

  it("keeps tag editing disabled after details failure and retries loading separately", async () => {
    const asset = createAsset(7);
    const onRetryTagDetails = vi.fn();
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([7]),
          selectedAssets: [asset],
          orderedAssetIds: [7],
          tagMode: "single",
          tagDetailsFailed: true,
          onRetryTagDetails
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.getByText("The complete tag list could not be loaded. Editing remains disabled.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retry loading tags" }));
    expect(onRetryTagDetails).toHaveBeenCalledTimes(1);
  });

  it("shows the assigned-tag loading state", () => {
    const asset = createAsset(7);
    render(
      <BulkActionsSidebar
        controller={createController({
          selectedAssetIds: new Set([7]),
          selectedAssets: [asset],
          orderedAssetIds: [7],
          tagMode: "single",
          singleAssetTags: ["cat"],
          tagDetailsLoading: true
        })}
        thumbs={{}}
        renderingThumbnailIds={{}}
      />
    );

    expect(screen.getByTestId("bulk-tag-list")).toHaveTextContent("Loading tags...");
    expect(screen.queryByText("cat")).not.toBeInTheDocument();
  });
});

function sortingController(overrides: Partial<BulkSelectionController> = {}) {
  return createController({
    selectedAssetIds: new Set([1, 2, 3]),
    selectedAssets: [createAsset(1), createAsset(2), createAsset(3)],
    orderedAssetIds: [1, 2, 3], groupKeyDraft: "trip", ...overrides
  });
}

async function openSorter(controller = sortingController()) {
  const view = render(<UiLayerProvider><BulkActionsSidebar controller={controller} thumbs={{}} renderingThumbnailIds={{}} /></UiLayerProvider>);
  const trigger = screen.getByRole("button", { name: "Sort in larger view" });
  await userEvent.click(trigger);
  return { ...view, trigger, controller };
}

function modalOrder() {
  return screen.getAllByTestId(/^bulk-order-tile-/).map(tile => Number(tile.getAttribute("data-sort-asset")));
}

describe("bulk group sorting modal", () => {
  afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.useRealTimers(); });

  it("keeps keyboard changes local, discards on Cancel and restores trigger focus", async () => {
    const { controller, trigger } = await openSorter();
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" });
    expect(modalOrder()).toEqual([2, 1, 3]);
    expect(screen.getByTestId("bulk-order-handle-1")).toHaveFocus();
    expect(controller.onReorderGroupAsset).not.toHaveBeenCalled();
    expect(controller.onApplyGroup).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
    await userEvent.click(trigger);
    expect(modalOrder()).toEqual([1, 2, 3]);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("retains the sorted closing frame, removes it after exit and initializes a fresh draft on reopening", async () => {
    const { trigger } = await openSorter();
    vi.useFakeTimers();
    fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByTestId("bulk-order-modal")).toHaveAttribute("data-modal-presence", "closing");
    expect(modalOrder()).toEqual([2, 1, 3]);
    act(() => vi.advanceTimersByTime(100));
    expect(screen.queryByTestId("bulk-order-modal")).toBeNull();
    fireEvent.click(trigger);
    expect(modalOrder()).toEqual([1, 2, 3]);
  });

  it("saves the exact modal draft, blocks duplicate submissions and dismissal while saving", async () => {
    let finish!: (value: Awaited<ReturnType<BulkSelectionController["onApplyGroup"]>>) => void;
    const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>(() => new Promise(resolve => { finish = resolve; }));
    await openSorter(sortingController({ onApplyGroup }));
    fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" });
    await userEvent.click(screen.getByRole("button", { name: "Save order" }));
    expect(onApplyGroup).toHaveBeenCalledExactlyOnceWith([2, 1, 3]);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByTestId("bulk-order-handle-1")).toBeDisabled();
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.mouseDown(screen.getByTestId("bulk-order-modal"));
    expect(screen.getByRole("dialog")).toBeVisible();
    await act(async () => finish({ status: "saved" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("retains the draft after failure and partial saves, then retries", async () => {
    const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>()
      .mockResolvedValueOnce({ status: "failed" })
      .mockResolvedValueOnce({ status: "partial", processed: 2, requested: 3 })
      .mockResolvedValueOnce({ status: "saved" });
    await openSorter(sortingController({ onApplyGroup }));
    fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" });
    await userEvent.click(screen.getByRole("button", { name: "Save order" }));
    expect(screen.getByRole("alert")).toBeVisible();
    expect(modalOrder()).toEqual([2, 1, 3]);
    await userEvent.click(screen.getByRole("button", { name: "Save order" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Closing keeps changes already saved.");
    await userEvent.click(screen.getByRole("button", { name: "Save order" }));
    expect(onApplyGroup).toHaveBeenLastCalledWith([2, 1, 3]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("ignores a retired group's save completion after another group opens", async () => {
    let finish!: (value: Awaited<ReturnType<BulkSelectionController["onApplyGroup"]>>) => void;
    const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>(() => new Promise(resolve => { finish = resolve; }));
    const { rerender, controller, trigger } = await openSorter(sortingController({ onApplyGroup }));
    fireEvent.click(screen.getByRole("button", { name: "Save order" }));
    rerender(<UiLayerProvider><BulkActionsSidebar controller={{ ...controller, groupKeyDraft: "other" }} thumbs={{}} renderingThumbnailIds={{}} /></UiLayerProvider>);
    fireEvent.click(trigger);
    await act(async () => finish({ status: "saved" }));
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Save order" })).toBeEnabled();
  });

  it("invalidates the modal when its group context changes", async () => {
    const { rerender, controller } = await openSorter();
    rerender(<UiLayerProvider><BulkActionsSidebar controller={{ ...controller, groupKeyDraft: "other" }} thumbs={{}} renderingThumbnailIds={{}} /></UiLayerProvider>);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(controller.onApplyGroup).not.toHaveBeenCalled();
  });

  it("disables opening until selected metadata is ready", () => {
    render(<BulkActionsSidebar controller={sortingController({ metadataLoading: true })} thumbs={{}} renderingThumbnailIds={{}} />);
    expect(screen.getByRole("button", { name: "Sort in larger view" })).toBeDisabled();
  });

  it("bounds mounted tiles and thumbnail requests for a thousand selected items", async () => {
    const assets = Array.from({ length: 1000 }, (_, index) => createAsset(index + 1));
    const queueThumbnailsByIds = vi.fn();
    await openSorter(sortingController({ selectedAssetIds: new Set(assets.map(asset => asset.id)), selectedAssets: assets,
      orderedAssetIds: assets.map(asset => asset.id), queueThumbnailsByIds }));
    expect(modalOrder().length).toBeLessThan(40);
    expect(queueThumbnailsByIds).toHaveBeenCalled();
    expect(queueThumbnailsByIds.mock.calls.every(([ids]) => ids.length < 40)).toBe(true);
  });
});
