import { UiLayerProvider } from "../../../ui/overlays/UiLayerProvider";
import { cleanup, fireEvent, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import type { BulkController } from "../createBulk";
type SelectionFields = "selectedCount" | "selectionKey" | "selectionRanged" | "selectionNotice";
/** Fakes derive the selection count and identity from `selectedAssetIds` unless given. */
type BulkSelectionController = Omit<BulkController, SelectionFields> &
  Partial<Pick<BulkController, SelectionFields>>;
const derived: Record<SelectionFields, (target: BulkSelectionController) => unknown> = {
  selectedCount: (target) => target.selectedAssetIds.size,
  selectionKey: (target) => target.selectedAssetIds,
  selectionRanged: () => false,
  selectionNotice: () => null
};
const withSelection = (controller: BulkSelectionController) =>
  new Proxy(controller, {
    get(target, property, receiver) {
      if (property in derived && !Object.getOwnPropertyDescriptor(target, property))
        return derived[property as SelectionFields](target);
      return Reflect.get(target, property, receiver);
    }
  }) as BulkController;
import { settle as act } from "../../../test/settle";
import { renderSolid } from "../../../test/renderSolid";
import { SuggestionClient } from "../../search/suggestions/SuggestionClient";
import { onCleanup } from "solid-js";
import { BulkDock } from "../BulkDock";

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(960);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(560);
});
afterEach(() => vi.restoreAllMocks());

/** Asset IDs of `thumb://` image requests currently in the DOM. */
const thumbnailRequests = () =>
  [...document.querySelectorAll("img")]
    .map((image) => /^thumb:\/\/localhost\/(\d+)\?/.exec(image.getAttribute("src") ?? "")?.[1])
    .filter((id): id is string => id !== undefined)
    .map(Number);

function Sidebar(props: { controller: BulkSelectionController }) {
  const client = new SuggestionClient();
  onCleanup(() => client.dispose());
  return (
    <UiLayerProvider>
      <BulkDock controller={withSelection(props.controller)} client={client} />
    </UiLayerProvider>
  );
}

const tagsTab = () => screen.getByRole("tab", { name: "Tags" });
const openGroupTab = () => userEvent.click(screen.getByRole("tab", { name: "Group & order" }));

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
    thumb_version: "0123456789abcdef",
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
    selectionBusy: false,
    metadataLoading: false,
    metadataFailed: false,
    onRetryMetadata: vi.fn(),
    partialResult: null,
    selectedAssetIds: new Set(),
    selectedAssets: [],
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
    onBulkSelectionInteraction: vi.fn(),
    onGroupKeyDraftChange: vi.fn(),
    onApplyGroup: vi.fn<BulkSelectionController["onApplyGroup"]>(async () => ({ status: "saved" })),
    onAddTag: vi.fn(async () => true),
    onRemoveTag: vi.fn(async () => true),
    onRetryTagDetails: vi.fn(),
    ...override
  };
}

describe("BulkDock", () => {
  afterEach(cleanup);

  it("restores focus after a completed save's disabled state is committed away", async () => {
    let settle: (saved: boolean) => void = () => {};
    const pending = new Promise<boolean>((resolve) => {
      settle = resolve;
    });
    const controller = createController({
      selectedAssetIds: new Set([1]),
      tagMode: "single",
      onAddTag: vi.fn(() => pending)
    });
    const { rerender } = renderSolid(Sidebar, { controller: controller });
    await userEvent.type(screen.getByLabelText("Add tag"), "cat{Enter}");
    rerender({ controller: { ...controller, tagApplying: true } });
    await userEvent.click(tagsTab());
    await act(async () => {
      settle(true);
      await pending;
    });
    await act(async () => {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    });
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    rerender({ controller: controller });
    await waitFor(() => expect(screen.getByLabelText("Add tag")).toHaveFocus());
    expect(screen.getByLabelText("Add tag")).toHaveValue("");
  });

  it("explains an invalid tag, keeps the draft and does not submit it", async () => {
    const controller = createController({ selectedAssetIds: new Set([1]), tagMode: "single" });
    renderSolid(Sidebar, { controller });
    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "two;tags{Enter}");
    expect(screen.getByTestId("bulk-tag-invalid")).toHaveTextContent(
      "Tags cannot contain whitespace, commas, semicolons, or control characters."
    );
    expect(input).toHaveValue("two;tags");
    expect(controller.onAddTag).not.toHaveBeenCalled();
    await userEvent.type(input, "{Backspace}");
    expect(screen.queryByTestId("bulk-tag-invalid")).not.toBeInTheDocument();
  });

  it("shows header tag feedback and partial counts on the Group & order tab", async () => {
    const controller = createController({
      selectedAssetIds: new Set([1, 2]),
      tagMode: "multiple",
      tagSaveFailed: true,
      partialResult: { processed: 1, requested: 2 }
    });
    renderSolid(Sidebar, { controller });
    await openGroupTab();
    expect(screen.getByText("Updated 1 of 2 selected items.")).toBeInTheDocument();
    expect(screen.getByText("The tag change was not saved. Try again.")).toBeInTheDocument();
    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "two;tags{Enter}");
    expect(screen.getByTestId("bulk-tag-invalid")).toBeInTheDocument();
    expect(controller.onAddTag).not.toHaveBeenCalled();
  });

  it("shows a partial tag write on the default Tags tab", () => {
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([1, 2]),
        tagMode: "multiple",
        partialResult: { processed: 1, requested: 2 }
      })
    });
    expect(tagsTab()).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("Updated 1 of 2 selected items.")).toBeInTheDocument();
  });

  it("cancels deferred tag focus when selection changes before re-enabling", async () => {
    let settle: (saved: boolean) => void = () => {};
    const pending = new Promise<boolean>((resolve) => {
      settle = resolve;
    });
    const controller = createController({
      selectedAssetIds: new Set([1]),
      tagMode: "single",
      onAddTag: vi.fn(() => pending)
    });
    const { rerender } = renderSolid(Sidebar, { controller: controller });
    await userEvent.type(screen.getByLabelText("Add tag"), "cat{Enter}");
    rerender({ controller: { ...controller, tagApplying: true } });
    await act(async () => {
      settle(true);
      await pending;
    });
    rerender({ controller: { ...controller, selectedAssetIds: new Set([2]) } });
    const groupInput = tagsTab();
    await userEvent.click(groupInput);
    await act(async () => {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    });
    expect(groupInput).toHaveFocus();
    expect(screen.getByLabelText("Add tag")).toHaveValue("");
  });

  it.each([
    "saved",
    "failed",
    "rejected"
  ] as const)("ignores a retired selection's %s tag save without clearing the new draft or stealing focus", async (outcome) => {
    let settle: (saved: boolean) => void = () => {};
    let reject: (error: Error) => void = () => {};
    const pending = new Promise<boolean>((resolve, rejectPromise) => {
      settle = resolve;
      reject = rejectPromise;
    });
    const controller = createController({
      selectedAssetIds: new Set([1]),
      tagMode: "single",
      onAddTag: vi.fn(() => pending)
    });
    const { rerender } = renderSolid(Sidebar, { controller: controller });
    await userEvent.type(screen.getByLabelText("Add tag"), "old-tag{Enter}");
    expect(controller.onAddTag).toHaveBeenCalledExactlyOnceWith("old-tag");
    rerender({ controller: { ...controller, selectedAssetIds: new Set([2]) } });
    expect(screen.getByLabelText("Add tag")).toHaveValue("");
    await userEvent.type(screen.getByLabelText("Add tag"), "new-draft");
    const groupInput = tagsTab();
    await userEvent.click(groupInput);
    await act(async () => {
      if (outcome === "rejected") reject(new Error("retired save"));
      else settle(outcome === "saved");
      await pending.catch(() => {});
    });
    // Flush the frame used for focus restoration, not merely the promise callback.
    await act(async () => {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    });
    expect(screen.getByLabelText("Add tag")).toHaveValue("new-draft");
    expect(groupInput).toHaveFocus();
    expect(controller.onAddTag).toHaveBeenCalledTimes(1);
  });

  it("explains why a range selection was dropped: changed results or an expired session", () => {
    renderSolid(Sidebar, { controller: createController() });
    expect(screen.queryByTestId("bulk-selection-cleared")).not.toBeInTheDocument();
    cleanup();

    renderSolid(Sidebar, { controller: createController({ selectionNotice: "rangeCleared" }) });
    const cleared = screen.getByTestId("bulk-selection-cleared");
    expect(cleared).toHaveTextContent("Selection cleared");
    expect(cleared).toHaveTextContent("The results changed");
    expect(cleared).not.toHaveTextContent("expired");
    cleanup();

    renderSolid(Sidebar, { controller: createController({ selectionNotice: "rangeExpired" }) });
    const expired = screen.getByTestId("bulk-selection-cleared");
    expect(expired).toHaveTextContent("Selection cleared");
    expect(expired).toHaveTextContent(
      "The selection expired before it could be used. Select the items again."
    );
    expect(expired).not.toHaveTextContent("The results changed");
  });

  it("renders the lightbox-style heart and exposes pending and failure states", async () => {
    const onToggleFavorite = vi.fn(async () => {});
    const { rerender } = renderSolid(Sidebar, { controller: createController() });
    expect(
      screen.queryByRole("button", { name: "Toggle favorites for selected items" })
    ).not.toBeInTheDocument();
    const controller = createController({ selectedAssetIds: new Set([1]), onToggleFavorite });
    rerender({ controller: controller });
    const heart = screen.getByRole("button", { name: "Toggle favorites for selected items" });
    expect(heart).toBeEnabled(); // Selected IDs need not have loaded gallery rows.
    await userEvent.click(heart);
    expect(onToggleFavorite).toHaveBeenCalledOnce();
    rerender({ controller: { ...controller, allSelectedFavorites: true, favoriteApplying: true } });
    expect(
      screen.getByRole("button", { name: "Remove selected items from favorites" })
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Remove selected items from favorites" })
    ).toHaveAttribute("aria-pressed", "true");
    rerender({ controller: { ...controller, favoriteFailed: true } });
    expect(screen.getByText("Could not update favorites. Try again.")).toBeVisible();
  });

  it("disables every write control while rectangle IDs are resolving", async () => {
    const controller = createController({
      selectionBusy: true,
      selectedAssetIds: new Set([1]),
      selectedAssets: [createAsset(1)],
      tagMode: "single",
      singleAssetTags: ["cat"]
    });
    renderSolid(Sidebar, { controller: controller });
    expect(
      screen.getByRole("button", { name: "Toggle favorites for selected items" })
    ).toBeDisabled();
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove tag cat" })).toBeDisabled();
    await openGroupTab();
    expect(screen.getByLabelText("Media group key")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove from group" })).toBeDisabled();
  });

  it("collapses to a chip for an empty selection and expands once items are selected", () => {
    const { rerender } = renderSolid(Sidebar, { controller: createController() });
    const dock = screen.getByTestId("bulk-action-panel");
    expect(dock).toHaveClass("bulk-dock");
    expect(dock).not.toHaveClass("bulk-dock--expanded");
    expect(dock).toHaveAttribute("data-selected-count", "0");
    expect(screen.getByTestId("bulk-dock-chip")).toHaveTextContent("Select items");
    expect(screen.queryByLabelText("Add tag")).not.toBeInTheDocument();
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Most used tags" })).not.toBeInTheDocument();

    rerender({ controller: createController({ selectedAssetIds: new Set([1, 2]), tagMode: "multiple" }) });
    expect(dock).toHaveClass("bulk-dock--expanded");
    expect(dock).not.toHaveClass("bulk-dock--tall");
    expect(dock).toHaveAttribute("data-selected-count", "2");
    expect(screen.queryByTestId("bulk-dock-chip")).not.toBeInTheDocument();
    const header = screen.getByTestId("bulk-header-panel");
    // The tag input comes first, then the heart and the clearing counter on the same row.
    expect([...header.children].map((child) => child.getAttribute("data-testid") ?? child.tagName)).toEqual([
      "bulk-tag-list",
      "BUTTON",
      "bulk-clear-selection"
    ]);
    expect(screen.getByLabelText("Add tag")).toBeEnabled();
    expect(tagsTab()).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toBe(screen.getByTestId("bulk-tags-panel"));
  });

  it("clears the selection from the counter", async () => {
    const controller = createController({ selectedAssetIds: new Set([1, 2, 3]) });
    renderSolid(Sidebar, { controller });
    const counter = screen.getByRole("button", { name: "Clear selection (3 selected)" });
    expect(counter).toHaveTextContent("3");
    await userEvent.click(counter);
    expect(controller.onBulkSelectionInteraction).toHaveBeenCalledExactlyOnceWith({ type: "clear" });
  });

  it("switches tabs by click and arrow keys and makes the group tab tall", async () => {
    renderSolid(Sidebar, { controller: createController({ selectedAssetIds: new Set([1]) }) });
    const dock = screen.getByTestId("bulk-action-panel");
    tagsTab().focus();
    await userEvent.keyboard("{ArrowRight}");
    const group = screen.getByRole("tab", { name: "Group & order" });
    expect(group).toHaveFocus();
    expect(group).toHaveAttribute("aria-selected", "true");
    expect(group).toHaveAttribute("tabindex", "0");
    expect(tagsTab()).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("tabpanel")).toBe(screen.getByTestId("bulk-group-panel"));
    expect(dock).toHaveClass("bulk-dock--tall");
    await userEvent.keyboard("{ArrowLeft}");
    expect(tagsTab()).toHaveFocus();
    expect(screen.getByTestId("bulk-tags-panel")).toBeInTheDocument();
    await openGroupTab();
    expect(screen.getByLabelText("Media group key")).toBeEnabled();
  });

  it("returns to the tags tab when the selection is cleared", async () => {
    const controller = createController({ selectedAssetIds: new Set([1]) });
    const { rerender } = renderSolid(Sidebar, { controller });
    await openGroupTab();
    rerender({ controller: { ...controller, selectedAssetIds: new Set() } });
    expect(screen.getByTestId("bulk-dock-chip")).toBeInTheDocument();
    rerender({ controller });
    expect(tagsTab()).toHaveAttribute("aria-selected", "true");
  });

  it("keeps the dock open to show why an empty selection was cleared", () => {
    renderSolid(Sidebar, { controller: createController({ selectionNotice: "rangeCleared" }) });
    expect(screen.getByTestId("bulk-action-panel")).toHaveClass("bulk-dock--expanded");
    expect(screen.getByTestId("bulk-selection-cleared")).toBeVisible();
    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.getByTestId("bulk-clear-selection")).toBeDisabled();
  });

  it("sorts inline and applies the changed order with the group", async () => {
    const controller = sortingController();
    renderSolid(Sidebar, { controller });
    expect(thumbnailRequests()).toEqual([]);
    await openGroupTab();
    expect(screen.getByTestId("bulk-inline-order-grid")).toBeInTheDocument();
    expect(thumbnailRequests()).toEqual([1, 2, 3]);
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-inline-order-handle-1"), { key: "ArrowRight" }));
    expect(inlineOrder()).toEqual([2, 1, 3]);
    await waitFor(() => expect(screen.getByTestId("bulk-inline-order-handle-1")).toHaveFocus());
    expect(controller.onApplyGroup).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Apply group" }));
    expect(controller.onApplyGroup).toHaveBeenCalledExactlyOnceWith([2, 1, 3]);
  });

  it("applies the controller's order when the inline order is unchanged", async () => {
    const controller = sortingController();
    renderSolid(Sidebar, { controller });
    await openGroupTab();
    await userEvent.click(screen.getByRole("button", { name: "Apply group" }));
    expect(controller.onApplyGroup).toHaveBeenCalledExactlyOnceWith(undefined);
  });

  it("opens the larger view from the inline draft and hides the inline board meanwhile", async () => {
    const controller = sortingController();
    renderSolid(Sidebar, { controller });
    await openGroupTab();
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-inline-order-handle-3"), { key: "ArrowLeft" }));
    expect(inlineOrder()).toEqual([1, 3, 2]);
    await userEvent.click(screen.getByRole("button", { name: "Sort in larger view" }));
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    expect(modalOrder()).toEqual([1, 3, 2]);
    expect(screen.queryByTestId("bulk-inline-order-grid")).not.toBeInTheDocument();
  });

  it("resets the inline draft when the group key changes", async () => {
    const controller = sortingController();
    const { rerender } = renderSolid(Sidebar, { controller });
    await openGroupTab();
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-inline-order-handle-1"), { key: "ArrowRight" }));
    expect(inlineOrder()).toEqual([2, 1, 3]);
    rerender({ controller: { ...controller, groupKeyDraft: "other" } });
    expect(inlineOrder()).toEqual([1, 2, 3]);
  });

  it("shows conflicts in the group tab and opens the larger sorting view", async () => {
    const controller = sortingController({ hasConflictingGroups: true });
    renderSolid(Sidebar, { controller: controller });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await openGroupTab();
    expect(screen.getByRole("alert")).toHaveTextContent("Multiple group assignments");
    expect(screen.getByTestId("bulk-group-order-panel")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sort in larger view" }));
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    expect(modalOrder()).toEqual([1, 2, 3]);
  });

  it("does not expose group ordering until a group key is set", async () => {
    const assets = [createAsset(1), createAsset(2)];
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([1, 2]),
        selectedAssets: assets,
        orderedAssetIds: [1, 2],
        groupKeyDraft: "",
        tagMode: "multiple"
      })
    });
    await openGroupTab();

    expect(screen.queryByTestId("bulk-group-order-panel")).not.toBeInTheDocument();
    expect(screen.queryByTestId("bulk-group-drag-handle-1")).not.toBeInTheDocument();
  });

  it("removes and automatically adds tags for a single selected asset", async () => {
    const asset = createAsset(7);
    const onAddTag = vi.fn(async () => true);
    const onRemoveTag = vi.fn(async () => true);
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([7]),
        selectedAssets: [asset],
        orderedAssetIds: [7],
        tagMode: "single",
        singleAssetTags: ["cat"],
        onAddTag,
        onRemoveTag
      })
    });

    expect(screen.queryByTestId("bulk-group-order-panel")).not.toBeInTheDocument();
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
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([1]),
        selectedAssets: [asset],
        orderedAssetIds: [1],
        tagMode: "single",
        onAddTag
      })
    });

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

    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([1]),
        selectedAssets: [asset],
        orderedAssetIds: [1],
        tagMode: "single",
        onAddTag
      })
    });

    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "travel{Enter}");
    await userEvent.click(tagsTab());
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
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([1, 2]),
        selectedAssets: assets,
        orderedAssetIds: [1, 2],
        tagMode: "multiple",
        appliedBulkTags: ["travel"]
      })
    });

    expect(screen.getByText("travel")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove tag travel" })).not.toBeInTheDocument();
  });

  it("keeps tag editing disabled after details failure and retries loading separately", async () => {
    const asset = createAsset(7);
    const onRetryTagDetails = vi.fn();
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([7]),
        selectedAssets: [asset],
        orderedAssetIds: [7],
        tagMode: "single",
        tagDetailsFailed: true,
        onRetryTagDetails
      })
    });

    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(
      screen.getByText("The complete tag list could not be loaded. Editing remains disabled.")
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retry loading tags" }));
    expect(onRetryTagDetails).toHaveBeenCalledTimes(1);
  });

  it("shows the assigned-tag loading state", () => {
    const asset = createAsset(7);
    renderSolid(Sidebar, {
      controller: createController({
        selectedAssetIds: new Set([7]),
        selectedAssets: [asset],
        orderedAssetIds: [7],
        tagMode: "single",
        singleAssetTags: ["cat"],
        tagDetailsLoading: true
      })
    });

    expect(screen.getByTestId("bulk-tag-list")).toHaveTextContent("Loading tags...");
    expect(screen.queryByText("cat")).not.toBeInTheDocument();
  });
});

function sortingController(overrides: Partial<BulkSelectionController> = {}): BulkSelectionController {
  return createController({
    selectedAssetIds: new Set([1, 2, 3]),
    selectedAssets: [createAsset(1), createAsset(2), createAsset(3)],
    orderedAssetIds: [1, 2, 3],
    groupKeyDraft: "trip",
    ...overrides
  });
}

async function openSorter(controller = sortingController()) {
  const view = renderSolid(Sidebar, { controller: controller });
  await openGroupTab();
  const trigger = screen.getByRole("button", { name: "Sort in larger view" });
  await userEvent.click(trigger);
  return { ...view, trigger, controller };
}

function inlineOrder() {
  return screen
    .getAllByTestId(/^bulk-inline-order-tile-/)
    .map((tile) => Number(tile.getAttribute("data-sort-asset")));
}

function modalOrder() {
  return screen
    .getAllByTestId(/^bulk-order-tile-/)
    .map((tile) => Number(tile.getAttribute("data-sort-asset")));
}

describe("bulk group sorting modal", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("keeps keyboard changes local, discards on Cancel and restores trigger focus", async () => {
    const { controller, trigger } = await openSorter();
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" }));
    expect(modalOrder()).toEqual([2, 1, 3]);
    await waitFor(() => expect(screen.getByTestId("bulk-order-handle-1")).toHaveFocus());
    expect(controller.onApplyGroup).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
    await userEvent.click(trigger);
    expect(modalOrder()).toEqual([1, 2, 3]);
    act(() => fireEvent.keyDown(document, { key: "Escape" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("retains the sorted closing frame, removes it after exit and initializes a fresh draft on reopening", async () => {
    const { trigger } = await openSorter();
    vi.useFakeTimers();
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" }));
    act(() => fireEvent.click(screen.getByRole("button", { name: "Cancel" })));
    expect(screen.getByTestId("bulk-order-modal")).toHaveAttribute(
      "data-modal-presence",
      "closing"
    );
    expect(modalOrder()).toEqual([2, 1, 3]);
    act(() => vi.advanceTimersByTime(100));
    expect(screen.queryByTestId("bulk-order-modal")).toBeNull();
    act(() => fireEvent.click(trigger));
    expect(modalOrder()).toEqual([1, 2, 3]);
  });

  it("saves the exact modal draft, blocks duplicate submissions and dismissal while saving", async () => {
    let finish!: (value: Awaited<ReturnType<BulkSelectionController["onApplyGroup"]>>) => void;
    const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    await openSorter(sortingController({ onApplyGroup }));
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" }));
    await userEvent.click(screen.getByRole("button", { name: "Save order" }));
    expect(onApplyGroup).toHaveBeenCalledExactlyOnceWith([2, 1, 3]);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByTestId("bulk-order-handle-1")).toBeDisabled();
    act(() => fireEvent.keyDown(document, { key: "Escape" }));
    act(() => fireEvent.mouseDown(screen.getByTestId("bulk-order-modal")));
    expect(screen.getByRole("dialog")).toBeVisible();
    await act(async () => finish({ status: "saved" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("retains the draft after failure and partial saves, then retries", async () => {
    const onApplyGroup = vi
      .fn<BulkSelectionController["onApplyGroup"]>()
      .mockResolvedValueOnce({ status: "failed" })
      .mockResolvedValueOnce({ status: "partial", processed: 2, requested: 3 })
      .mockResolvedValueOnce({ status: "saved" });
    await openSorter(sortingController({ onApplyGroup }));
    act(() => fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" }));
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
    const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const { rerender, controller, trigger } = await openSorter(sortingController({ onApplyGroup }));
    act(() => fireEvent.click(screen.getByRole("button", { name: "Save order" })));
    rerender({ controller: { ...controller, groupKeyDraft: "other" } });
    act(() => fireEvent.click(trigger));
    await act(async () => finish({ status: "saved" }));
    expect(screen.getByRole("dialog", { name: "Sort group order" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Save order" })).toBeEnabled();
  });

  it("invalidates the modal when its group context changes", async () => {
    const { rerender, controller } = await openSorter();
    rerender({ controller: { ...controller, groupKeyDraft: "other" } });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(controller.onApplyGroup).not.toHaveBeenCalled();
  });

  it("disables opening until selected metadata is ready", async () => {
    renderSolid(Sidebar, { controller: sortingController({ metadataLoading: true }) });
    await openGroupTab();
    expect(screen.getByRole("button", { name: "Sort in larger view" })).toBeDisabled();
  });

  it("bounds mounted tiles and thumbnail requests for a thousand selected items", async () => {
    const assets = Array.from({ length: 1000 }, (_, index) => createAsset(index + 1));
    await openSorter(
      sortingController({
        selectedAssetIds: new Set(assets.map((asset) => asset.id)),
        selectedAssets: assets,
        orderedAssetIds: assets.map((asset) => asset.id)
      })
    );
    expect(modalOrder().length).toBeLessThan(40);
    // Only mounted tiles request a `thumb://` URL.
    expect(thumbnailRequests().length).toBeGreaterThan(0);
    expect(thumbnailRequests().length).toBeLessThan(40);
  });
});

it("keeps the active modal save when shared metadata changes during acknowledgement", async () => {
  let finish!: (result: Awaited<ReturnType<BulkSelectionController["onApplyGroup"]>>) => void;
  const onApplyGroup = vi.fn<BulkSelectionController["onApplyGroup"]>(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      })
  );
  const { rerender, controller } = await openSorter(sortingController({ onApplyGroup }));
  act(() => fireEvent.keyDown(screen.getByTestId("bulk-order-handle-1"), { key: "ArrowRight" }));
  await userEvent.click(screen.getByRole("button", { name: "Save order" }));
  rerender({
    controller: {
      ...controller,
      groupApplying: true,
      selectedAssets: controller.selectedAssets.map((asset) => ({
        ...asset,
        media_group_key: "trip"
      }))
    }
  });
  expect(modalOrder()).toEqual([2, 1, 3]);
  await act(async () => finish({ status: "saved" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
