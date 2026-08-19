import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset } from "../../../../types";
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

function createAsset(id: number): Asset {
  return {
    id,
    path: `C:/media/${id}.jpg`,
    kind: "image",
    size_bytes: 150,
    modified_at: 10,
    width: 320,
    height: 240,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: []
  };
}

describe("saveLightboxMediaGroupAction", () => {
  beforeEach(() => {
    apiMocks.setAssetMediaGroup.mockReset();
  });

  it("saves media group without mutating editors directly", async () => {
    apiMocks.setAssetMediaGroup.mockResolvedValue(undefined);

    let assets = [createAsset(1), createAsset(2)];
    let selected: Asset | null = createAsset(2);

    const setAssets = vi.fn((updater: (previous: Asset[]) => Asset[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: Asset | null) => Asset | null) => {
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
