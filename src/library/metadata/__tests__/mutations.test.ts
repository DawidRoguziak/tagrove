import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LanguageProvider } from "../../../i18n/solid";
import { createAppRuntime } from "../../../app/createAppRuntime";
import { createLibrary } from "../../createLibrary";
import { settle } from "../../../test/settle";
import type { AssetDetails, AssetQueryReady } from "../../../platform/tauri/contracts/assets";
import type { SetAssetTagsSummary } from "../../../platform/tauri/contracts/tags";

const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetDetails: vi.fn(),
  getAssetQueryPage: vi.fn(),
  setAssetFavorite: vi.fn(),
  toggleAssetsFavoriteBulk: vi.fn(),
  setAssetMediaGroup: vi.fn(),
  setAssetsMediaGroupBulk: vi.fn(),
  deleteAsset: vi.fn(),
  setAssetTags: vi.fn(),
  mergeAssetTagsBulk: vi.fn(),
  listTags: vi.fn()
}));
vi.mock("../../../platform/tauri/assets", () => api);
vi.mock("../../../platform/tauri/tags", () => api);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
}
function detail(id: number): AssetDetails {
  return {
    id,
    file_name: `${id}.jpg`,
    path: `/synthetic/${id}.jpg`,
    size_bytes: 10,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: ["cat"]
  };
}
function ready(): AssetQueryReady {
  const { path: _path, size_bytes: _size, tags: _tags, ...summary } = detail(1);
  return { status: "ready", session_id: 1, revision: 1, offset: 0, total: 1, items: [summary] };
}
function saved(tags = ["dog"]): SetAssetTagsSummary {
  return {
    asset_id: 1,
    changed: true,
    tags,
    vocabulary_changed: false,
    query_impact: { type: "none" },
    revision: 2
  };
}
async function setup() {
  const view = renderHook(() => createLibrary());
  await settle(() => view.result.refreshLibrary());
  await view.result.details.load(1);
  return view;
}
beforeEach(() => {
  vi.resetAllMocks();
  api.startAssetQuery.mockResolvedValue(ready());
  api.getAssetDetails.mockImplementation(async (id) => detail(id));
  api.listTags.mockResolvedValue({ items: ["cat", "dog"], total: 2 });
  api.setAssetTags.mockResolvedValue(saved());
});

describe("shared canonical metadata publication", () => {
  it("keeps one library across Settings navigation and starts queries with 128-item pages", async () => {
    const { result: runtime } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
    const library = runtime.library;
    await settle(() => library.query.refresh());
    settle(() => runtime.openSettings());
    expect(runtime.view()).toBe("settings");
    settle(() => runtime.closeSettings());
    expect(runtime.library).toBe(library);
    expect(runtime.library.query.assets).toHaveLength(1);
    expect(api.startAssetQuery).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ pageSize: 128 })
    );
  });

  it("rejects same-turn duplicate writes and publishes canonical tags only after backend success", async () => {
    const { result: library } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const oldRead = library.tags.captureGeneration(1);
    const notifications = vi.fn();
    library.mutations.subscribe(notifications);
    const write = library.mutations.saveTags(1, ["DOG"]);
    await expect(library.mutations.setFavorite(1, true)).rejects.toThrow("Metadata is busy");
    expect(api.setAssetFavorite).not.toHaveBeenCalled();
    expect(library.tags.get(1)?.tags).toEqual(["cat"]);
    expect(library.details.get(1)).toBeNull();
    expect(notifications).not.toHaveBeenCalled();
    const during = library.tags.captureGeneration(1);
    pending.resolve(saved());
    await settle(() => write);
    expect(library.tags.get(1)?.tags).toEqual(["dog"]);
    expect(library.details.get(1)?.tags).toEqual(["dog"]);
    expect(library.query.getAssetAt(0)).not.toHaveProperty("tags");
    expect(library.tags.publishDetails(1, ["old"], oldRead)).toBe(false);
    expect(library.tags.publishDetails(1, ["during"], during)).toBe(false);
    expect(notifications).toHaveBeenCalledOnce();
    expect(api.startAssetQuery).toHaveBeenCalledOnce();
    expect(api.listTags).toHaveBeenCalledOnce();
  });

  it("keeps canonical state on rejection, releases ownership and allows retry", async () => {
    const { result: library } = await setup();
    api.setAssetTags.mockRejectedValueOnce(new Error("locked"));
    await expect(library.mutations.saveTags(1, ["dog"])).rejects.toThrow("locked");
    expect(library.tags.get(1)?.tags).toEqual(["cat"]);
    expect(library.details.get(1)).toBeNull();
    await library.mutations.saveTags(1, ["dog"]);
    expect(library.tags.get(1)?.tags).toEqual(["dog"]);
  });

  it("acquires bulk ownership all-or-none and settles a partial canonical result", async () => {
    const { result: library } = await setup();
    const held = library.tags.beginMutation(2)!;
    await expect(library.mutations.mergeTags([1, 2], ["dog"])).rejects.toThrow("busy");
    expect(api.mergeAssetTagsBulk).not.toHaveBeenCalled();
    library.tags.settleMutation(held);
    api.mergeAssetTagsBulk.mockResolvedValue({
      results: [{ asset_id: 1, tags: ["cat", "dog"], changed: true }],
      processed_assets: 1,
      vocabulary_changed: false,
      query_impact: { type: "none" }
    });
    await settle(() => library.mutations.mergeTags([1, 2], ["dog"]));
    expect(library.tags.get(1)?.tags).toEqual(["cat", "dog"]);
    expect(library.tags.get(2)).toBeNull();
    expect(api.startAssetQuery).toHaveBeenCalledTimes(2);
    expect(library.tags.beginMutation(2)).not.toBeNull();
  });

  it("uses the current applied filter when a save settles", async () => {
    const { result: library } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const write = library.mutations.saveTags(1, ["dog"]);
    await settle(() =>
      library.applyFilters({
        filterInclude: ["dog"],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );
    pending.resolve({
      ...saved(),
      query_impact: { type: "tags", changed_tags: ["dog"], tag_count_changed: false }
    });
    await settle(() => write);
    expect(api.startAssetQuery).toHaveBeenCalledTimes(3);
  });

  it("drains active writes before maintenance, blocks new admissions synchronously, resets even on failure", async () => {
    const { result: library } = await setup();
    const pending = deferred<void>();
    api.setAssetFavorite.mockReturnValueOnce(pending.promise);
    const write = library.mutations.setFavorite(1, true);
    const maintenance = vi.fn(async () => {
      throw new Error("restore failed");
    });
    const barrier = library.runMaintenance("identity", maintenance);
    const rejection = expect(barrier).rejects.toThrow("restore failed");
    await expect(library.mutations.setFavorite(2, true)).rejects.toThrow("busy");
    await Promise.resolve();
    expect(maintenance).not.toHaveBeenCalled();
    pending.resolve();
    await write;
    await settle(() => rejection);
    expect(maintenance).toHaveBeenCalledOnce();
    expect(library.query.assets).toEqual([]);
    expect(library.tags.get(1)).toBeNull();
    expect(library.details.get(1)).toBeNull();
    await library.mutations.setFavorite(1, true);
    expect(api.setAssetFavorite).toHaveBeenCalledTimes(2);
  });

  it("does not publish a stale identity or a disposed owner's write and releases waiting maintenance", async () => {
    const { result: library, cleanup } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const write = library.mutations.saveTags(1, ["dog"]);
    library.handleImportDbRestored();
    await library.details.load(1);
    const maintenance = library.runMaintenance("metadata", async () => {});
    const rejection = expect(maintenance).rejects.toThrow("unavailable");
    cleanup();
    await rejection;
    pending.resolve(saved());
    await write;
    expect(library.tags.get(1)?.tags).toEqual(["cat"]);
    expect(api.listTags).toHaveBeenCalledOnce();
  });

  it("patches favorites in query and details and publishes only acknowledged bulk IDs", async () => {
    const { result: library } = await setup();
    await settle(() => library.mutations.setFavorite(1, true));
    expect(library.query.getAssetAt(0)?.is_favorite).toBe(true);
    expect(library.details.get(1)?.is_favorite).toBe(true);
    expect(api.startAssetQuery).toHaveBeenCalledOnce();
    api.toggleAssetsFavoriteBulk.mockResolvedValue({
      processed_asset_ids: [1],
      is_favorite: false,
      revision: 3
    });
    await settle(() => library.mutations.toggleFavorites([1, 2]));
    expect(api.startAssetQuery).toHaveBeenCalledTimes(2);
    expect(library.tags.get(2)).toBeNull();
  });

  it("refreshes favorites-only queries after removal and bulk toggles", async () => {
    const { result: library } = await setup();
    await library.applyFilters({
      filterInclude: [],
      filterExclude: [],
      appliedMediaKind: "all",
      appliedFavoritesOnly: true
    });
    await library.mutations.setFavorite(1, false);
    api.toggleAssetsFavoriteBulk.mockResolvedValue({ processed_asset_ids: [1], is_favorite: true });
    await library.mutations.toggleFavorites([1]);
    expect(api.startAssetQuery).toHaveBeenCalledTimes(4);
  });

  it("waits for the single-favorite refresh and reports its failure after publishing durable success", async () => {
    const { result: library } = await setup();
    await library.applyFilters({
      filterInclude: [],
      filterExclude: [],
      appliedMediaKind: "all",
      appliedFavoritesOnly: true
    });
    const refresh = deferred<AssetQueryReady>();
    api.startAssetQuery.mockReturnValueOnce(refresh.promise);
    const finished = vi.fn();
    const write = library.mutations.setFavorite(1, false).then(finished);
    const rejection = expect(write).rejects.toThrow("refresh failed");
    await waitFor(() => expect(api.startAssetQuery).toHaveBeenCalledTimes(3));
    expect(finished).not.toHaveBeenCalled();
    expect(library.query.getAssetAt(0)?.is_favorite).toBe(false);
    refresh.reject(new Error("refresh failed"));
    await rejection;
    expect(library.tags.beginMutation(1)).not.toBeNull();
  });

  it("publishes normalized groups and uses canonical bulk acknowledgement", async () => {
    const { result: library } = await setup();
    const commits: unknown[] = [];
    library.mutations.subscribe((commit) => commits.push(commit));
    await library.mutations.setMediaGroup(1, " Trip ", 2);
    expect(api.setAssetMediaGroup).toHaveBeenCalledWith(1, "Trip", 2);
    api.setAssetsMediaGroupBulk.mockResolvedValue({
      processed_asset_ids: [1],
      media_group_key: "Trip",
      processed_assets: 1,
      updated_assets: 1
    });
    await library.mutations.setMediaGroups(
      [
        { assetId: 1, mediaGroupOrder: 3 },
        { assetId: 2, mediaGroupOrder: 4 }
      ],
      " Trip "
    );
    expect(commits).toEqual([
      {
        type: "updated",
        patches: new Map([[1, { media_group_key: "Trip", media_group_order: 2 }]])
      },
      {
        type: "updated",
        patches: new Map([[1, { media_group_key: "Trip", media_group_order: 3 }]])
      }
    ]);
    expect(api.startAssetQuery).toHaveBeenCalledTimes(3);
  });

  it("keeps deletion tombstones and prevents old detail reads from resurrecting the asset", async () => {
    const { result: library } = await setup();
    const old = library.tags.captureGeneration(1);
    const refresh = deferred<AssetQueryReady>();
    api.startAssetQuery.mockReturnValueOnce(refresh.promise);
    const deletion = library.mutations.deleteAsset(1);
    await waitFor(() => expect(library.query.getAssetAt(0)).toBeUndefined());
    expect(library.query.getAssetAt(0)).toBeUndefined();
    expect(library.tags.publishDetails(1, ["old"], old)).toBe(false);
    expect(library.details.get(1)).toBeNull();
    refresh.resolve({ ...ready(), items: [], total: 0 });
    await settle(() => deletion);
    await waitFor(() => expect(library.query.total).toBe(0));
  });
});

describe("details cache identity and bounds", () => {
  it("deduplicates reads, retains exactly 256 details and reloads evicted records", async () => {
    const { result: library } = await setup();
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    const first = library.details.load(2);
    expect(library.details.load(2)).toBe(first);
    pending.resolve(detail(2));
    await first;
    for (let id = 3; id <= 257; id++) await library.details.load(id);
    expect(library.details.get(1)).toBeNull();
    expect(library.details.get(2)?.id).toBe(2);
    expect(
      Array.from({ length: 257 }, (_, i) => library.details.get(i + 1)).filter(Boolean)
    ).toHaveLength(256);
    await library.details.load(1);
    expect(api.getAssetDetails).toHaveBeenCalledTimes(258);
  });

  it.each([
    "query",
    "identity",
    "clear",
    "dispose"
  ])("rejects a pending detail after %s invalidation", async (mode) => {
    const { result: library, cleanup } = await setup();
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    const read = library.details.load(2);
    if (mode === "query") await library.query.refresh();
    else if (mode === "identity") library.handleImportDbRestored();
    else if (mode === "clear") library.handleLibraryCleared();
    else cleanup();
    pending.resolve(detail(2));
    expect(await read).toBeNull();
    expect(library.details.get(2)).toBeNull();
  });

  it("rejects details read before or during writes and allows a fresh read after settlement", async () => {
    const { result: library } = await setup();
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    const read = library.details.load(2);
    await library.mutations.setFavorite(2, true);
    pending.resolve(detail(2));
    expect(await read).toBeNull();
    api.getAssetDetails.mockResolvedValueOnce({ ...detail(2), is_favorite: true });
    expect((await library.details.load(2))?.is_favorite).toBe(true);
  });
});

it("keeps vocabulary until restore refresh but clears it immediately on library clear", async () => {
  const { result: library } = await setup();
  settle(() => library.thumbnails.thumbnailStore.complete({ 1: "/synthetic/thumb.jpg" }, []));
  expect(library.vocabulary.knownTags).toEqual(["cat", "dog"]);
  settle(library.handleImportDbRestored);
  expect(library.tags.get(1)).toBeNull();
  expect(library.details.get(1)).toBeNull();
  expect(library.query.total).toBe(0);
  expect(library.thumbnails.thumbnailStore.getPath(1)).toBeUndefined();
  expect(library.vocabulary.knownTags).toEqual(["cat", "dog"]);
  settle(library.handleLibraryCleared);
  expect(library.vocabulary.knownTags).toEqual([]);
});
