import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetDetails, SelectedAsset } from "../../../../types";
import { saveLightboxTagsAction } from "../saveLightboxTagsAction";

const apiMocks = vi.hoisted(() => ({
  setAssetTags: vi.fn()
}));

vi.mock("../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../api")>("../../../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createDetails(id: number, tags: string[] = []): AssetDetails {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    path: `C:/media/${id}.jpg`,
    kind: "image",
    size_bytes: 128,
    modified_at: 100,
    width: 320,
    height: 240,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags
  };
}

describe("saveLightboxTagsAction", () => {
  beforeEach(() => {
    apiMocks.setAssetTags.mockReset();
  });

  it("saves tags and updates the selected asset view", async () => {
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({ asset_id: assetId, changed: true, tags, query_impact: { type: "tags", changed_tags: ["cat", "dog", "travel"], tag_count_changed: true }, revision: 2 }));
    const refreshKnownTags = vi.fn().mockResolvedValue(["cat", "dog"]);

    let selected: SelectedAsset = createDetails(2, ["old"]);
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected)!;
    });

    await saveLightboxTagsAction(
      {
        selected,
        tagEditor: ["ignored"],
        setSelected,
        refreshKnownTags
      },
      ["cat", "dog"]
    );

    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(2, ["cat", "dog"]);
    expect(selected.tags).toEqual(["cat", "dog"]);
    expect(refreshKnownTags).toHaveBeenCalledTimes(1);
  });

  it("normalizes tags from tag editor when explicit tags are not provided", async () => {
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({ asset_id: assetId, changed: true, tags, query_impact: { type: "tags", changed_tags: ["cat", "dog", "travel"], tag_count_changed: true }, revision: 2 }));
    const refreshKnownTags = vi.fn().mockResolvedValue(["cat", "dog"]);

    let selected: SelectedAsset = createDetails(5, ["old"]);
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected)!;
    });

    await saveLightboxTagsAction({
      selected,
      tagEditor: ["  Cat ", "dog", "cat"],
      setSelected,
      refreshKnownTags
    });

    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(5, ["cat", "dog"]);
    expect(selected.tags).toEqual(["cat", "dog"]);
    expect(refreshKnownTags).toHaveBeenCalledTimes(1);
  });

  it("does not patch stale local identity when the coordinator rejects the result", async () => {
    apiMocks.setAssetTags.mockResolvedValue({ asset_id: 8, changed: true, tags: ["new"], query_impact: { type: "tags", changed_tags: ["cat", "dog", "travel"], tag_count_changed: true }, revision: 2 });
    const setSelected = vi.fn();
    const refreshKnownTags = vi.fn().mockResolvedValue([]);
    const onSaved = vi.fn(() => false);

    await saveLightboxTagsAction({
      selected: createDetails(8, ["old"]),
      tagEditor: ["new"],
      setSelected,
      refreshKnownTags,
      onSaved
    });

    expect(onSaved).toHaveBeenCalledWith(8, ["new"]);
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
  });

  it("does nothing when nothing is selected", async () => {
    const refreshKnownTags = vi.fn().mockResolvedValue([]);
    const setSelected = vi.fn();

    await saveLightboxTagsAction({
      selected: null,
      tagEditor: ["cat"],
      setSelected,
      refreshKnownTags
    });

    expect(apiMocks.setAssetTags).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
  });
});
