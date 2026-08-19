import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset } from "../../../../types";
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

function createAsset(id: number, tags: string[] = []): Asset {
  return {
    id,
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

  it("saves tags and updates local list + selected", async () => {
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({ asset_id: assetId, changed: true, tags, revision: 2 }));
    const refreshKnownTags = vi.fn().mockResolvedValue(["cat", "dog"]);

    let assets = [createAsset(1, []), createAsset(2, ["old"])];
    let selected: Asset | null = createAsset(2, ["old"]);

    const setAssets = vi.fn((updater: (previous: Asset[]) => Asset[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: Asset | null) => Asset | null) => {
      selected = updater(selected);
    });

    await saveLightboxTagsAction(
      {
        selected,
        tagEditor: ["ignored"],
        setAssets,
        setSelected,
        refreshKnownTags
      },
      ["cat", "dog"]
    );

    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(2, ["cat", "dog"]);
    expect(assets[1]?.tags).toEqual(["cat", "dog"]);
    expect(selected?.tags).toEqual(["cat", "dog"]);
    expect(refreshKnownTags).toHaveBeenCalledTimes(1);
  });

  it("normalizes tags from tag editor when explicit tags are not provided", async () => {
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({ asset_id: assetId, changed: true, tags, revision: 2 }));
    const refreshKnownTags = vi.fn().mockResolvedValue(["cat", "dog"]);

    let assets = [createAsset(5, ["old"])];
    let selected: Asset | null = createAsset(5, ["old"]);

    const setAssets = vi.fn((updater: (previous: Asset[]) => Asset[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: Asset | null) => Asset | null) => {
      selected = updater(selected);
    });

    await saveLightboxTagsAction({
      selected,
      tagEditor: ["  Cat ", "dog", "cat"],
      setAssets,
      setSelected,
      refreshKnownTags
    });

    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(5, ["cat", "dog"]);
    expect(assets[0]?.tags).toEqual(["cat", "dog"]);
    expect(selected?.tags).toEqual(["cat", "dog"]);
    expect(refreshKnownTags).toHaveBeenCalledTimes(1);
  });

  it("does not patch stale local identity when the coordinator rejects the result", async () => {
    apiMocks.setAssetTags.mockResolvedValue({ asset_id: 8, changed: true, tags: ["new"], revision: 2 });
    const setAssets = vi.fn();
    const setSelected = vi.fn();
    const refreshKnownTags = vi.fn().mockResolvedValue([]);
    const onSaved = vi.fn(() => false);

    await saveLightboxTagsAction({
      selected: createAsset(8, ["old"]),
      tagEditor: ["new"],
      setAssets,
      setSelected,
      refreshKnownTags,
      onSaved
    });

    expect(onSaved).toHaveBeenCalledWith(8, ["new"]);
    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
  });

  it("does nothing when nothing is selected", async () => {
    const refreshKnownTags = vi.fn().mockResolvedValue([]);
    const setAssets = vi.fn();
    const setSelected = vi.fn();

    await saveLightboxTagsAction({
      selected: null,
      tagEditor: ["cat"],
      setAssets,
      setSelected,
      refreshKnownTags
    });

    expect(apiMocks.setAssetTags).not.toHaveBeenCalled();
    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
  });
});
