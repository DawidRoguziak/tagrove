import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary, SelectedAsset } from "../../../../types";
import { saveLightboxMediaGroupAction } from "../saveLightboxMediaGroupAction";

const apiMocks = vi.hoisted(() => ({
  setAssetMediaGroup: vi.fn()
}));

vi.mock("../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../api")>("../../../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createSummary(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 10,
    width: 320,
    height: 240,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}

function createAsset(id: number): SelectedAsset {
  return {
    ...createSummary(id),
    path: `C:/media/${id}.jpg`,
    size_bytes: 150,
    tags: []
  };
}

describe("saveLightboxMediaGroupAction", () => {
  beforeEach(() => {
    apiMocks.setAssetMediaGroup.mockReset();
  });

  it("saves media group without mutating editors directly", async () => {
    apiMocks.setAssetMediaGroup.mockResolvedValue(undefined);

    let assets = [createSummary(1), createSummary(2)];
    let selected: SelectedAsset | null = createAsset(2);

    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected);
    });
    const setMediaGroupKeyEditor = vi.fn();
    const setMediaGroupOrderEditor = vi.fn();

    await saveLightboxMediaGroupAction(
      {
        selected,
        setAssets,
        setSelected,
        setMediaGroupKeyEditor,
        setMediaGroupOrderEditor
      },
      { key: "group-a", order: 3.5 }
    );

    expect(apiMocks.setAssetMediaGroup).toHaveBeenCalledWith(2, "group-a", 3.5);
    expect(assets[1]?.media_group_key).toBe("group-a");
    expect(assets[1]?.media_group_order).toBe(3.5);
    expect(selected?.media_group_key).toBe("group-a");
    expect(selected?.media_group_order).toBe(3.5);
    expect(setMediaGroupKeyEditor).not.toHaveBeenCalled();
    expect(setMediaGroupOrderEditor).not.toHaveBeenCalled();
  });

  it("does nothing when nothing is selected", async () => {
    const setAssets = vi.fn();
    const setSelected = vi.fn();
    const setMediaGroupKeyEditor = vi.fn();
    const setMediaGroupOrderEditor = vi.fn();

    await saveLightboxMediaGroupAction(
      {
        selected: null,
        setAssets,
        setSelected,
        setMediaGroupKeyEditor,
        setMediaGroupOrderEditor
      },
      { key: null, order: null }
    );

    expect(apiMocks.setAssetMediaGroup).not.toHaveBeenCalled();
    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(setMediaGroupKeyEditor).not.toHaveBeenCalled();
    expect(setMediaGroupOrderEditor).not.toHaveBeenCalled();
  });
});
