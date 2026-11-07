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
    thumb_version: "0123456789abcdef",
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
const flush = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
/** No admitted write remains: a fresh write starts without waiting. */
async function expectIdle(library: ReturnType<typeof createLibrary>) {
  const turn = await library.tags.enqueueMutation([1, 2]);
  expect(turn.begin()).toHaveLength(2);
  turn.release();
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

  it("queues a favorite behind a pending tag save and publishes both canonical results in order", async () => {
    const { result: library } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    api.setAssetFavorite.mockResolvedValue(undefined);
    const oldRead = library.tags.captureGeneration(1);
    const notifications = vi.fn();
    library.mutations.subscribe(notifications);
    const write = library.mutations.saveTags(1, ["DOG"]);
    const favorite = library.mutations.setFavorite(1, true);
    await flush();
    expect(api.setAssetTags).toHaveBeenCalledOnce();
    expect(api.setAssetFavorite).not.toHaveBeenCalled();
    expect(library.tags.get(1)?.tags).toEqual(["cat"]);
    expect(library.details.get(1)).toBeNull();
    expect(notifications).not.toHaveBeenCalled();
    const during = library.tags.captureGeneration(1);
    pending.resolve(saved());
    await settle(() => Promise.all([write, favorite]));
    expect(api.setAssetFavorite).toHaveBeenCalledExactlyOnceWith(1, true);
    expect(api.setAssetTags.mock.invocationCallOrder[0]).toBeLessThan(
      api.setAssetFavorite.mock.invocationCallOrder[0]
    );
    expect(library.tags.get(1)?.tags).toEqual(["dog"]);
    // The favorite overlays the details its predecessor published, not an older snapshot.
    expect(library.details.get(1)).toMatchObject({ tags: ["dog"], is_favorite: true });
    expect(library.query.getAssetAt(0)).not.toHaveProperty("tags");
    expect(library.query.getAssetAt(0)?.is_favorite).toBe(true);
    expect(library.tags.publishDetails(1, ["old"], oldRead)).toBe(false);
    expect(library.tags.publishDetails(1, ["during"], during)).toBe(false);
    expect(notifications).toHaveBeenCalledTimes(2);
    expect(api.startAssetQuery).toHaveBeenCalledOnce();
    expect(api.listTags).toHaveBeenCalledOnce();
  });

  it("runs the next queued write after a failure and computes queued replacements from canonical tags", async () => {
    const { result: library } = await setup();
    const failing = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(failing.promise);
    api.setAssetTags.mockImplementation(async (_id: number, tags: string[]) => saved(tags));
    const first = library.mutations.saveTags(1, ["lost"]);
    const second = library.mutations.saveTags(1, (canonical) => [...canonical, "dog"]);
    await flush();
    expect(api.setAssetTags).toHaveBeenCalledOnce();
    failing.reject(new Error("locked"));
    await expect(first).rejects.toThrow("locked");
    await settle(() => second);
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "dog"]);
    const third = library.mutations.saveTags(1, (canonical) => [...canonical, "bird"]);
    await settle(() => third);
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "dog", "bird"]);
    expect(library.tags.get(1)?.tags).toEqual(["cat", "dog", "bird"]);
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

  it("waits for every bulk target, never sends a reduced subset and settles a partial canonical result", async () => {
    const { result: library } = await setup();
    const held = await library.tags.enqueueMutation([2]);
    const [token] = held.begin();
    api.mergeAssetTagsBulk.mockResolvedValue({
      results: [{ asset_id: 1, tags: ["cat", "dog"], changed: true }],
      processed_assets: 1,
      vocabulary_changed: false,
      query_impact: { type: "none" }
    });
    const merge = library.mutations.mergeTags([1, 2], ["dog"]);
    await flush();
    expect(api.mergeAssetTagsBulk).not.toHaveBeenCalled();
    library.tags.settleMutation(token);
    held.release();
    await settle(() => merge);
    expect(api.mergeAssetTagsBulk).toHaveBeenCalledExactlyOnceWith([1, 2], ["dog"]);
    expect(library.tags.get(1)?.tags).toEqual(["cat", "dog"]);
    expect(library.tags.get(2)).toBeNull();
    expect(api.startAssetQuery).toHaveBeenCalledTimes(2);
    await expectIdle(library);
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

  it("drains active and queued writes before maintenance, blocks new admissions synchronously, resets even on failure", async () => {
    const { result: library } = await setup();
    const pending = deferred<void>();
    api.setAssetFavorite.mockReturnValueOnce(pending.promise);
    const write = library.mutations.setFavorite(1, true);
    api.setAssetMediaGroup.mockResolvedValue(undefined);
    const queued = library.mutations.setMediaGroup(1, "trip", 1);
    await flush();
    const maintenance = vi.fn(async () => {
      throw new Error("restore failed");
    });
    const barrier = library.runMaintenance("identity", maintenance);
    const rejection = expect(barrier).rejects.toThrow("restore failed");
    await expect(library.mutations.setFavorite(2, true)).rejects.toThrow(
      "paused during maintenance"
    );
    await Promise.resolve();
    expect(maintenance).not.toHaveBeenCalled();
    pending.resolve();
    await write;
    await queued;
    expect(api.setAssetMediaGroup).toHaveBeenCalledExactlyOnceWith(1, "trip", 1);
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

  it("does not restore captured details or tags into a reused identity", async () => {
    const { result: library } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const write = library.mutations.saveTags(1, ["obsolete"]);
    library.handleImportDbRestored();
    const current = { ...detail(1), path: "/synthetic/reused.jpg", size_bytes: 99, tags: ["current"] };
    api.getAssetDetails.mockResolvedValueOnce(current);
    await library.details.load(1);
    pending.resolve(saved(["obsolete"]));
    await settle(() => write);
    expect(library.details.get(1)).toEqual(current);
    expect(library.tags.get(1)?.tags).toEqual(["current"]);
  });

  it("publishes saved tags without restoring an evicted complete record", async () => {
    const { result: library } = await setup();
    for (let id = 2; id <= 257; id++) await library.details.load(id);
    expect(library.details.get(1)).toBeNull();
    await settle(() => library.mutations.saveTags(1, ["dog"]));
    expect(library.tags.get(1)?.tags).toEqual(["dog"]);
    expect(library.details.get(1)).toBeNull();
    const current = { ...detail(1), tags: ["dog"] };
    api.getAssetDetails.mockResolvedValueOnce(current);
    expect(await library.details.load(1)).toEqual(current);
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

  it("settles a saved favorite independently of its failed query refresh", async () => {
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
    await settle(() => write);
    await waitFor(() => expect(api.startAssetQuery).toHaveBeenCalledTimes(3));
    expect(finished).toHaveBeenCalledOnce();
    expect(library.query.getAssetAt(0)?.is_favorite).toBe(false);
    refresh.reject(new Error("refresh failed"));
    await waitFor(() => expect(library.query.loadError).toBe("refresh failed"));
    await expectIdle(library);
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

  it("rejects a write queued behind its asset's deletion without sending IPC", async () => {
    const { result: library } = await setup();
    const deletion = deferred<unknown>();
    api.deleteAsset.mockReturnValueOnce(deletion.promise);
    const removing = library.mutations.deleteAsset(1);
    const favorite = library.mutations.setFavorite(1, true);
    const rejection = expect(favorite).rejects.toThrow("deleted");
    deletion.resolve({
      removed_assets: 1,
      removed_thumbnails: 0,
      source_status: "deleted",
      revision: 2,
      recovery_path: null
    });
    await rejection;
    await settle(() => removing);
    expect(api.setAssetFavorite).not.toHaveBeenCalled();
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
  expect(library.vocabulary.knownTags).toEqual(["cat", "dog"]);
  settle(library.handleImportDbRestored);
  expect(library.tags.get(1)).toBeNull();
  expect(library.details.get(1)).toBeNull();
  expect(library.query.total).toBe(0);
  expect(library.vocabulary.knownTags).toEqual(["cat", "dog"]);
  settle(library.handleLibraryCleared);
  expect(library.vocabulary.knownTags).toEqual([]);
});
