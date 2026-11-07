import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { createLibrary } from "../../../library/createLibrary";
import { createSelection } from "../../gallery/createSelection";
import { createBulk } from "../createBulk";
import { createSelectedSummaries } from "../createSelectedSummaries";
import { settle } from "../../../test/settle";
import type { AssetDetails, BulkFavoriteSummary } from "../../../platform/tauri/contracts/assets";
import type { SetAssetTagsSummary } from "../../../platform/tauri/contracts/tags";
const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetDetails: vi.fn(),
  getAssetSummariesByIds: vi.fn(),
  setAssetTags: vi.fn(),
  mergeAssetTagsBulk: vi.fn(),
  toggleAssetsFavoriteBulk: vi.fn(),
  setAssetsMediaGroupBulk: vi.fn(),
  applySelectionMutation: vi.fn(),
  listTags: vi.fn()
}));
vi.mock("../../../platform/tauri/assets", () => api);
vi.mock("../../../platform/tauri/tags", () => api);
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: Error) => void;
  const promise = new Promise<T>((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
function asset(id: number, overrides: Partial<AssetDetails> = {}): AssetDetails {
  return {
    id,
    file_name: `${id}.png`,
    path: `/synthetic/${id}.png`,
    size_bytes: 10,
    kind: "image",
    preview_path: null,
    modified_at: 1,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_version: "0123456789abcdef",
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: ["cat"],
    ...overrides
  };
}
const saved = (id: number, tags: string[]): SetAssetTagsSummary => ({
  asset_id: id,
  tags,
  changed: true,
  query_impact: { type: "none" },
  vocabulary_changed: false,
  revision: 2
});
beforeEach(() => {
  vi.resetAllMocks();
  api.startAssetQuery.mockResolvedValue({
    status: "ready",
    session_id: 1,
    revision: 1,
    offset: 0,
    total: 2,
    items: [asset(1), asset(2)]
  });
  api.getAssetDetails.mockImplementation(async (id) => asset(id));
  api.getAssetSummariesByIds.mockImplementation(async (ids) => ids.map((id: number) => asset(id)));
  api.listTags.mockResolvedValue({ items: ["cat", "dog"], total: 2 });
  api.setAssetTags.mockImplementation(async (id, tags) => saved(id, tags));
  api.mergeAssetTagsBulk.mockImplementation(async (ids: number[], tags: string[]) => ({
    processed_assets: ids.length,
    processed_asset_ids: ids,
    updated_assets: ids.length,
    updated_asset_ids: ids,
    results: ids.map((id) => ({ asset_id: id, tags: ["cat", ...tags], changed: true })),
    query_impact: { type: "none" },
    vocabulary_changed: false,
    revision: 2
  }));
  api.toggleAssetsFavoriteBulk.mockImplementation(async (ids) => ({
    processed_asset_ids: ids,
    is_favorite: true,
    revision: 2
  }));
  api.setAssetsMediaGroupBulk.mockImplementation(async (updates, key) => ({
    processed_asset_ids: updates.map((x: { assetId: number }) => x.assetId),
    processed_assets: updates.length,
    updated_assets: updates.length,
    media_group_key: key
  }));
});
async function setup(ids = [1]) {
  const view = renderHook(() => {
    const library = createLibrary();
    const selection = createSelection({
      queryEpoch: () => library.query.queryEpoch,
      identityEpoch: () => library.tags.epoch,
      getIdsRange: library.query.getIdsRangeAsync,
      sessionId: library.query.sessionId,
      total: () => library.query.total
    });
    const [settings, setSettings] = createSignal(false);
    const bulk = createBulk(library, selection, settings);
    return { library, selection, bulk, setSettings };
  });
  const { library, selection, bulk } = view.result;
  await settle(() => library.refreshLibrary());
  settle(() => selection.toggle());
  for (const id of ids)
    settle(() =>
      selection.interact({
        type: "click",
        assetId: id,
        assetIndex: id - 1,
        ctrlLike: false,
        shift: false
      })
    );
  await waitFor(() => {
    expect(bulk.metadataLoading).toBe(false);
    expect(bulk.tagDetailsLoading).toBe(false);
  });
  return { ...view, ...view.result };
}
function replace(selection: ReturnType<typeof createSelection>, id: number) {
  settle(() => {
    selection.clear();
    selection.interact({
      type: "click",
      assetId: id,
      assetIndex: id - 1,
      ctrlLike: false,
      shift: false
    });
  });
}
describe("bulk canonical editing", () => {
  it("loads full tags, normalizes additions, removes and shares canonical data", async () => {
    const { bulk, library } = await setup();
    expect(bulk.singleAssetTags).toEqual(["cat"]);
    expect(await settle(() => bulk.onAddTag(" DOG "))).toBe(true);
    expect(api.setAssetTags).toHaveBeenCalledWith(1, ["cat", "dog"]);
    await settle(() => bulk.onRemoveTag("cat"));
    expect(library.tags.get(1)?.tags).toEqual(["dog"]);
    expect(bulk.singleAssetTags).toEqual(["dog"]);
    expect(api.listTags).toHaveBeenCalledTimes(1);
  });
  it("accepts unchanged additions without a write", async () => {
    const { bulk } = await setup();
    expect(await bulk.onAddTag("cat")).toBe(true);
    expect(api.setAssetTags).not.toHaveBeenCalled();
  });
  it("never replaces unknown tags and retries missing details separately", async () => {
    api.getAssetDetails.mockResolvedValueOnce(null);
    const { bulk } = await setup();
    expect(bulk.tagDetailsFailed).toBe(true);
    expect(await bulk.onAddTag("dog")).toBe(false);
    expect(api.setAssetTags).not.toHaveBeenCalled();
    settle(bulk.onRetryTagDetails);
    await waitFor(() => expect(bulk.singleAssetTags).toEqual(["cat"]));
    expect(bulk.tagDetailsFailed).toBe(false);
  });
  it("retains canonical tags after rejection and allows retry", async () => {
    const { bulk } = await setup();
    api.setAssetTags.mockRejectedValueOnce(new Error("disk"));
    expect(await settle(() => bulk.onAddTag("dog"))).toBe(false);
    expect(bulk.tagSaveFailed).toBe(true);
    expect(bulk.singleAssetTags).toEqual(["cat"]);
    expect(await settle(() => bulk.onAddTag("dog"))).toBe(true);
    expect(bulk.tagSaveFailed).toBe(false);
  });
  it("queues single-asset saves behind other writers and keeps every quick addition", async () => {
    const { bulk, library } = await setup();
    const external = await library.tags.enqueueMutation([1]);
    const [token] = external.begin();
    let queued!: Promise<boolean>;
    settle(() => {
      queued = bulk.onAddTag("dog");
    });
    expect(bulk.tagApplying).toBe(true);
    await Promise.resolve();
    expect(api.setAssetTags).not.toHaveBeenCalled();
    settle(() => library.tags.settleMutation(token, ["cat", "external"]));
    external.release();
    expect(await settle(() => queued)).toBe(true);
    // The replacement is computed from the canonical tags the earlier writer left behind.
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "external", "dog"]);
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const first = bulk.onAddTag("bird");
    const second = bulk.onAddTag("fish");
    pending.resolve(saved(1, ["cat", "external", "dog", "bird"]));
    expect(await settle(() => Promise.all([first, second]))).toEqual([true, true]);
    expect(api.setAssetTags).toHaveBeenCalledTimes(3);
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, [
      "cat",
      "external",
      "dog",
      "bird",
      "fish"
    ]);
    expect(bulk.singleAssetTags).toEqual(["cat", "external", "dog", "bird", "fish"]);
    expect(bulk.tagApplying).toBe(false);
  });
  it("does not publish old detail reads into a replacement selection", async () => {
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    const view = renderHook(() => {
      const library = createLibrary();
      const selection = createSelection({
        queryEpoch: () => 0,
        identityEpoch: () => library.tags.epoch,
        getIdsRange: async () => []
      });
      return { selection, bulk: createBulk(library, selection, () => false) };
    });
    settle(view.result.selection.toggle);
    replace(view.result.selection, 1);
    replace(view.result.selection, 2);
    await waitFor(() => expect(view.result.bulk.singleAssetTags).toEqual(["cat"]));
    pending.resolve(asset(1, { tags: ["old"] }));
    await settle(() => pending.promise);
    expect(view.result.bulk.singleAssetTags).toEqual(["cat"]);
  });
  it("merges all IDs including offscreen ones and shows add-only feedback", async () => {
    const { bulk, library } = await setup([1, 2000]);
    await settle(() => bulk.onAddTag("dog"));
    expect(api.mergeAssetTagsBulk).toHaveBeenCalledWith([1, 2000], ["dog"]);
    expect(bulk.appliedBulkTags).toEqual(["dog"]);
    expect(library.tags.get(2000)?.tags).toEqual(["cat", "dog"]);
    expect(bulk.selectedAssets.map((a) => a.id)).toEqual([1, 2000]);
    expect(api.getAssetDetails).toHaveBeenCalledTimes(1);
  });
  it("queues a merge behind a writer on one target and sends the complete ID set", async () => {
    const { bulk, library } = await setup([1, 2]);
    const external = await library.tags.enqueueMutation([2]);
    const [token] = external.begin();
    const merge = bulk.onAddTag("dog");
    await Promise.resolve();
    expect(api.mergeAssetTagsBulk).not.toHaveBeenCalled();
    settle(() => library.tags.settleMutation(token));
    external.release();
    expect(await settle(() => merge)).toBe(true);
    expect(api.mergeAssetTagsBulk).toHaveBeenCalledExactlyOnceWith([1, 2], ["dog"]);
  });
  it("runs a tag and a favorite on the same selection in click order", async () => {
    const { bulk } = await setup([1, 2]);
    const pending = deferred<unknown>();
    api.mergeAssetTagsBulk.mockReturnValueOnce(pending.promise);
    const tag = bulk.onAddTag("dog");
    const favorite = bulk.onToggleFavorite();
    await Promise.resolve();
    expect(api.toggleAssetsFavoriteBulk).not.toHaveBeenCalled();
    pending.resolve({
      processed_assets: 2,
      results: [
        { asset_id: 1, tags: ["cat", "dog"], changed: true },
        { asset_id: 2, tags: ["cat", "dog"], changed: true }
      ],
      query_impact: { type: "none" },
      vocabulary_changed: false
    });
    expect(await settle(() => tag)).toBe(true);
    await settle(() => favorite);
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledExactlyOnceWith([1, 2]);
    expect(api.mergeAssetTagsBulk.mock.invocationCallOrder[0]).toBeLessThan(
      api.toggleAssetsFavoriteBulk.mock.invocationCallOrder[0]
    );
    expect(bulk.tagSaveFailed).toBe(false);
    expect(bulk.favoriteFailed).toBe(false);
    expect(bulk.allSelectedFavorites).toBe(true);
  });
  it("ignores obsolete selection errors and bulk feedback", async () => {
    const { bulk, selection } = await setup([1, 2]);
    const pending = deferred<never>();
    api.mergeAssetTagsBulk.mockReturnValueOnce(pending.promise);
    const write = bulk.onAddTag("dog");
    replace(selection, 3);
    pending.reject(new Error("old"));
    await settle(() => write);
    expect(bulk.tagSaveFailed).toBe(false);
    expect(bulk.appliedBulkTags).toEqual([]);
  });
  it("reports a zero-result merge, keeps it retryable and refreshes", async () => {
    const { bulk } = await setup([1, 2]);
    api.mergeAssetTagsBulk.mockResolvedValueOnce({
      processed_assets: 0,
      results: [],
      query_impact: { type: "none" },
      vocabulary_changed: false
    });
    expect(await settle(() => bulk.onAddTag("dog"))).toBe(false);
    expect(bulk.tagSaveFailed).toBe(true);
    expect(bulk.partialResult).toEqual({ processed: 0, requested: 2 });
    expect(api.startAssetQuery).toHaveBeenCalledTimes(2);
  });
  it("blocks every operation while rectangle IDs are resolving", async () => {
    const { bulk, selection } = await setup();
    settle(() => selection.interact({ type: "rectangle-start" }));
    expect(await bulk.onAddTag("dog")).toBe(false);
    await bulk.onToggleFavorite();
    expect(await bulk.onApplyGroup()).toEqual({ status: "ignored" });
    expect(api.setAssetTags).not.toHaveBeenCalled();
    expect(api.toggleAssetsFavoriteBulk).not.toHaveBeenCalled();
  });
});
describe("bulk favorites and groups", () => {
  it("uses backend favorite results for mixed and all-favorite selections", async () => {
    const { bulk } = await setup([1, 2]);
    await settle(bulk.onToggleFavorite);
    expect(bulk.allSelectedFavorites).toBe(true);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({
      processed_asset_ids: [1, 2],
      is_favorite: false,
      revision: 3
    });
    await settle(bulk.onToggleFavorite);
    expect(bulk.allSelectedFavorites).toBe(false);
  });
  it("prunes only missing IDs, including an entirely missing selection", async () => {
    const { bulk, selection } = await setup([1, 2000]);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({
      processed_asset_ids: [2000],
      is_favorite: true,
      revision: 2
    });
    await settle(bulk.onToggleFavorite);
    expect([...selection.ids]).toEqual([2000]);
    expect(bulk.selectedAssets[0]?.is_favorite).toBe(true);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({
      processed_asset_ids: [],
      is_favorite: false,
      revision: 2
    });
    await settle(bulk.onToggleFavorite);
    expect(selection.ids.size).toBe(0);
  });
  it("refreshes favorites-only filters", async () => {
    const { bulk, library } = await setup([1, 2000]);
    await settle(() =>
      library.applyFilters({
        filterInclude: [],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: true
      })
    );
    await settle(bulk.onToggleFavorite);
    expect(api.startAssetQuery).toHaveBeenCalledTimes(3);
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledWith([1, 2000]);
  });
  it("keeps favorite failures in their card and retries", async () => {
    const { bulk } = await setup();
    api.toggleAssetsFavoriteBulk.mockRejectedValueOnce(new Error("disk"));
    await settle(bulk.onToggleFavorite);
    expect(bulk.favoriteFailed).toBe(true);
    expect(bulk.tagSaveFailed).toBe(false);
    expect(bulk.allSelectedFavorites).toBe(false);
    await settle(bulk.onToggleFavorite);
    expect(bulk.favoriteFailed).toBe(false);
  });
  it("queues repeated favorites and drains them before maintenance", async () => {
    const { bulk, library } = await setup();
    const pending = deferred<BulkFavoriteSummary>();
    api.toggleAssetsFavoriteBulk.mockReturnValueOnce(pending.promise);
    const first = bulk.onToggleFavorite();
    const second = bulk.onToggleFavorite();
    const maintenance = vi.fn();
    const barrier = library.runMaintenance("metadata", async () => maintenance());
    await Promise.resolve();
    expect(maintenance).not.toHaveBeenCalled();
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledOnce();
    pending.resolve({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
    await settle(() => Promise.all([first, second]));
    await settle(() => barrier);
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledTimes(2);
    expect(maintenance).toHaveBeenCalledOnce();
    expect(api.toggleAssetsFavoriteBulk.mock.invocationCallOrder[1]).toBeLessThan(
      maintenance.mock.invocationCallOrder[0]
    );
    expect(bulk.favoriteApplying).toBe(false);
  });
  it("ignores stale identity completions", async () => {
    const { bulk, library, selection } = await setup();
    const pending = deferred<BulkFavoriteSummary>();
    api.toggleAssetsFavoriteBulk.mockReturnValueOnce(pending.promise);
    const first = bulk.onToggleFavorite();
    settle(library.resetIdentity);
    replace(selection, 1);
    pending.resolve({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
    await settle(() => first);
    expect(bulk.allSelectedFavorites).toBe(false);
  });
  it("preserves existing single group order and normalizes the key", async () => {
    api.startAssetQuery.mockResolvedValueOnce({
      status: "ready",
      session_id: 1,
      revision: 1,
      offset: 0,
      total: 1,
      items: [asset(1, { media_group_key: "trip", media_group_order: 7 })]
    });
    const { bulk } = await setup();
    settle(() => bulk.onGroupKeyDraftChange(" TRIP "));
    expect(await settle(() => bulk.onApplyGroup())).toEqual({ status: "saved" });
    expect(api.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
      [{ assetId: 1, mediaGroupOrder: 7 }],
      "TRIP"
    );
  });
  it("applies the exact order, rejects incomplete orders and removes groups", async () => {
    const { bulk } = await setup([1, 2]);
    settle(() => bulk.onGroupKeyDraftChange(" trip "));
    expect(await bulk.onApplyGroup([1, 1])).toEqual({ status: "ignored" });
    expect(await bulk.onApplyGroup([2])).toEqual({ status: "ignored" });
    expect(await settle(() => bulk.onApplyGroup([2, 1]))).toEqual({ status: "saved" });
    expect(api.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
      [
        { assetId: 2, mediaGroupOrder: 1 },
        { assetId: 1, mediaGroupOrder: 2 }
      ],
      "trip"
    );
    settle(() => bulk.onGroupKeyDraftChange(""));
    await settle(() => bulk.onApplyGroup());
    expect(api.setAssetsMediaGroupBulk).toHaveBeenLastCalledWith(
      [
        { assetId: 2, mediaGroupOrder: null },
        { assetId: 1, mediaGroupOrder: null }
      ],
      null
    );
  });
  it("retains group drafts after failure and reports partial saves", async () => {
    const { bulk } = await setup([1, 2]);
    settle(() => bulk.onGroupKeyDraftChange("trip"));
    api.setAssetsMediaGroupBulk.mockRejectedValueOnce(new Error("disk"));
    expect(await settle(() => bulk.onApplyGroup())).toEqual({ status: "failed" });
    expect(bulk.groupKeyDraft).toBe("trip");
    expect(bulk.groupFailed).toBe(true);
    api.setAssetsMediaGroupBulk.mockResolvedValueOnce({
      processed_asset_ids: [1],
      processed_assets: 1,
      updated_assets: 1,
      media_group_key: "trip"
    });
    expect(await settle(() => bulk.onApplyGroup())).toEqual({
      status: "partial",
      processed: 1,
      requested: 2
    });
  });
});
describe("selected summary hydration", () => {
  it("batches at 256 with four requests and retains records across eviction", async () => {
    const pending = Array.from({ length: 5 }, () => deferred<AssetDetails[]>());
    api.getAssetSummariesByIds.mockImplementation(
      () => pending[api.getAssetSummariesByIds.mock.calls.length - 1].promise
    );
    const view = renderHook(() => {
      const [ids, setIds] = createSignal<ReadonlySet<number>>(
        new Set(Array.from({ length: 1025 }, (_, i) => i + 1))
      );
      const model = createSelectedSummaries({
        ids,
        cached: () => [],
        epoch: () => 0,
        subscribe: () => () => {}
      });
      return { model, setIds };
    });
    expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(4);
    expect(api.getAssetSummariesByIds.mock.calls.every(([ids]) => ids.length === 256)).toBe(true);
    pending[0].resolve(Array.from({ length: 256 }, (_, i) => asset(i + 1)));
    await settle(() => pending[0].promise);
    await waitFor(() => expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(5));
    for (let i = 1; i < 5; i++)
      pending[i].resolve(
        Array.from({ length: i === 4 ? 1 : 256 }, (_, j) => asset(i * 256 + j + 1))
      );
    await waitFor(() => expect(view.result.model.ready).toBe(true));
    settle(() => view.result.setIds(new Set([1, 1025])));
    expect(view.result.model.selected.map((a) => a.id)).toEqual([1, 1025]);
    expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(5);
  });
  it("marks omitted records retryable without dropping IDs", async () => {
    api.getAssetSummariesByIds.mockResolvedValueOnce([]);
    const { bulk, selection } = await setup([2000]);
    expect(bulk.metadataFailed).toBe(true);
    expect([...selection.ids]).toEqual([2000]);
    expect(await bulk.onApplyGroup()).toEqual({ status: "ignored" });
    settle(bulk.onRetryMetadata);
    await waitFor(() => expect(bulk.selectedAssets[0]?.id).toBe(2000));
    expect(bulk.metadataFailed).toBe(false);
  });
});

describe("bulk read and completion races", () => {
  it("overlays durable favorites on an older offscreen summary response", async () => {
    const pending = deferred<AssetDetails[]>();
    api.getAssetSummariesByIds.mockReturnValueOnce(pending.promise);
    const view = renderHook(() => {
      const library = createLibrary();
      const selection = createSelection({
        queryEpoch: () => 0,
        identityEpoch: () => library.tags.epoch,
        getIdsRange: async () => []
      });
      return { library, selection, bulk: createBulk(library, selection, () => false) };
    });
    settle(view.result.selection.toggle);
    replace(view.result.selection, 2000);
    await settle(view.result.bulk.onToggleFavorite);
    pending.resolve([asset(2000)]);
    await settle(() => pending.promise);
    await waitFor(() => expect(view.result.bulk.selectedAssets).toHaveLength(1));
    expect(view.result.bulk.allSelectedFavorites).toBe(true);
  });
  it("preserves a typed group key while offscreen metadata arrives", async () => {
    const pending = deferred<AssetDetails[]>();
    api.getAssetSummariesByIds.mockReturnValueOnce(pending.promise);
    const view = renderHook(() => {
      const library = createLibrary();
      const selection = createSelection({
        queryEpoch: () => 0,
        identityEpoch: () => 0,
        getIdsRange: async () => []
      });
      return { selection, bulk: createBulk(library, selection, () => false) };
    });
    settle(view.result.selection.toggle);
    replace(view.result.selection, 2000);
    settle(() => view.result.bulk.onGroupKeyDraftChange("typed"));
    pending.resolve([asset(2000, { media_group_key: "loaded" })]);
    await settle(() => pending.promise);
    expect(view.result.bulk.groupKeyDraft).toBe("typed");
    expect(view.result.bulk.orderedAssetIds).toEqual([2000]);
  });
  it("does not reset current group drafts or failures when a retired save settles", async () => {
    const { bulk, selection } = await setup([1, 2]);
    const pending = deferred<{
      processed_asset_ids: number[];
      processed_assets: number;
      updated_assets: number;
      media_group_key: string;
    }>();
    api.setAssetsMediaGroupBulk.mockReturnValueOnce(pending.promise);
    settle(() => bulk.onGroupKeyDraftChange("old"));
    const write = bulk.onApplyGroup();
    replace(selection, 3);
    settle(() => bulk.onGroupKeyDraftChange("new"));
    pending.resolve({
      processed_asset_ids: [1, 2],
      processed_assets: 2,
      updated_assets: 2,
      media_group_key: "old"
    });
    expect(await settle(() => write)).toEqual({ status: "ignored" });
    expect(bulk.groupKeyDraft).toBe("new");
    expect(bulk.groupFailed).toBe(false);
  });
  it("does not update a disposed controller when a request rejects", async () => {
    const { bulk, cleanup } = await setup();
    const pending = deferred<SetAssetTagsSummary>();
    api.setAssetTags.mockReturnValueOnce(pending.promise);
    const write = bulk.onAddTag("dog");
    cleanup();
    pending.reject(new Error("closed"));
    expect(await write).toBe(false);
  });
  it("ignores obsolete hydration and keeps requests bounded after selection replacement", async () => {
    const pending = deferred<AssetDetails[]>();
    api.getAssetSummariesByIds.mockReturnValueOnce(pending.promise);
    const view = renderHook(() => {
      const [ids, setIds] = createSignal<ReadonlySet<number>>(new Set([2000]));
      return {
        setIds,
        model: createSelectedSummaries({
          ids,
          cached: () => [],
          epoch: () => 0,
          subscribe: () => () => {}
        })
      };
    });
    settle(() => view.result.setIds(new Set([3000])));
    await waitFor(() => expect(view.result.model.selected[0]?.id).toBe(3000));
    pending.resolve([asset(2000)]);
    await settle(() => pending.promise);
    expect(view.result.model.selected.map((a) => a.id)).toEqual([3000]);
  });
});

describe("bulk partial publication and query changes", () => {
  it("publishes only returned tag lists and keeps durable feedback after refresh failure", async () => {
    const { bulk, library } = await setup([1, 2]);
    api.startAssetQuery.mockRejectedValueOnce(new Error("refresh"));
    api.mergeAssetTagsBulk.mockResolvedValueOnce({
      processed_assets: 1,
      results: [{ asset_id: 1, tags: ["canonical"] }],
      query_impact: { type: "all" },
      vocabulary_changed: false
    });
    expect(await settle(() => bulk.onAddTag("dog"))).toBe(true);
    expect(bulk.appliedBulkTags).toEqual(["dog"]);
    expect(bulk.partialResult).toEqual({ processed: 1, requested: 2 });
    expect(library.tags.get(1)?.tags).toEqual(["canonical"]);
    expect(library.tags.get(2)).toBeNull();
    const turn = await library.tags.enqueueMutation([2]);
    expect(turn.begin()).toHaveLength(1);
    turn.release();
  });
  it("keeps drafts and all selected IDs across filtered query replacement", async () => {
    const { bulk, library } = await setup([1, 2000]);
    settle(() => bulk.onGroupKeyDraftChange("kept"));
    await settle(() => bulk.onAddTag("dog"));
    // The filtered query is a new backend session.
    api.startAssetQuery.mockResolvedValueOnce({
      status: "ready",
      session_id: 2,
      revision: 2,
      offset: 0,
      total: 1,
      items: [asset(1)]
    });
    await settle(() =>
      library.applyFilters({
        filterInclude: ["dog"],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );
    expect(bulk.groupKeyDraft).toBe("kept");
    expect(bulk.appliedBulkTags).toEqual(["dog"]);
    await settle(() => bulk.onAddTag("bird"));
    expect(api.mergeAssetTagsBulk).toHaveBeenLastCalledWith([1, 2000], ["bird"]);
  });
  it("refreshes a matching tag filter after single removal", async () => {
    const { bulk, library } = await setup();
    await settle(() =>
      library.applyFilters({
        filterInclude: ["cat"],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );
    api.setAssetTags.mockResolvedValueOnce({
      ...saved(1, []),
      query_impact: { type: "tags", changed_tags: ["cat"], tag_count_changed: true }
    });
    await settle(() => bulk.onRemoveTag("cat"));
    expect(api.startAssetQuery).toHaveBeenCalledTimes(3);
  });
  it("ignores details completed after deletion and clears selection on identity reset", async () => {
    api.getAssetDetails.mockRejectedValueOnce(new Error("read"));
    const { bulk, library, selection } = await setup();
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    settle(bulk.onRetryTagDetails);
    library.tags.remove(1);
    pending.resolve(asset(1));
    await settle(() => pending.promise);
    expect(bulk.singleAssetTags).toEqual([]);
    await waitFor(() => expect(bulk.tagDetailsFailed).toBe(true));
    settle(library.resetIdentity);
    expect(selection.ids.size).toBe(0);
    expect(bulk.singleAssetTags).toEqual([]);
  });
  it("queues group saves behind another writer and repeated saves behind each other", async () => {
    const { bulk, library } = await setup([1, 2]);
    const external = await library.tags.enqueueMutation([2]);
    const [token] = external.begin();
    const pending = deferred<{
      processed_asset_ids: number[];
      processed_assets: number;
      updated_assets: number;
      media_group_key: null;
    }>();
    api.setAssetsMediaGroupBulk.mockReturnValueOnce(pending.promise);
    const first = bulk.onApplyGroup();
    const second = bulk.onApplyGroup();
    await Promise.resolve();
    expect(api.setAssetsMediaGroupBulk).not.toHaveBeenCalled();
    settle(() => library.tags.settleMutation(token));
    external.release();
    await waitFor(() => expect(api.setAssetsMediaGroupBulk).toHaveBeenCalledOnce());
    pending.resolve({
      processed_asset_ids: [1, 2],
      processed_assets: 2,
      updated_assets: 2,
      media_group_key: null
    });
    expect(await settle(() => first)).toEqual({ status: "saved" });
    expect(await settle(() => second)).toEqual({ status: "saved" });
    expect(api.setAssetsMediaGroupBulk).toHaveBeenCalledTimes(2);
    expect(bulk.groupApplying).toBe(false);
  });
  it("re-reads complete tags when a query refresh discards a pending detail read", async () => {
    const view = renderHook(() => {
      const library = createLibrary();
      const selection = createSelection({
        queryEpoch: () => library.query.queryEpoch,
        identityEpoch: () => library.tags.epoch,
        getIdsRange: library.query.getIdsRangeAsync
      });
      return { library, selection, bulk: createBulk(library, selection, () => false) };
    });
    const { library, selection, bulk } = view.result;
    await settle(() => library.refreshLibrary());
    const pending = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(pending.promise);
    settle(() => selection.toggle());
    settle(() =>
      selection.interact({
        type: "click",
        assetId: 1,
        assetIndex: 0,
        ctrlLike: false,
        shift: false
      })
    );
    await waitFor(() => expect(api.getAssetDetails).toHaveBeenCalledOnce());
    await settle(() => library.query.refresh());
    await settle(async () => pending.resolve(asset(1)));
    await waitFor(() => expect(bulk.singleAssetTags).toEqual(["cat"]));
    expect(bulk.tagDetailsFailed).toBe(false);
    expect(bulk.tagDetailsLoading).toBe(false);
    expect(api.getAssetDetails).toHaveBeenCalledTimes(2);
  });
});

it("enables the editor when another owner publishes canonical tags before an old read completes", async () => {
  api.getAssetDetails.mockRejectedValueOnce(new Error("read"));
  const { bulk, library } = await setup();
  const pending = deferred<AssetDetails>();
  api.getAssetDetails.mockReturnValueOnce(pending.promise);
  settle(bulk.onRetryTagDetails);
  expect(bulk.tagDetailsLoading).toBe(true);
  await settle(() => library.mutations.saveTags(1, ["shared"]));
  expect(bulk.singleAssetTags).toEqual(["shared"]);
  expect(bulk.tagDetailsLoading).toBe(false);
  expect(bulk.tagDetailsFailed).toBe(false);
  pending.resolve(asset(1));
  await settle(() => pending.promise);
  expect(bulk.singleAssetTags).toEqual(["shared"]);
});

it("ignores deletion notifications outside the selected IDs", () => {
  let notify: (commit: import("../../../library/metadata/createMutations").MetadataCommit) => void =
    () => {};
  const { result } = renderHook(() =>
    createSelectedSummaries({
      ids: () => new Set([1]),
      cached: () => [asset(1)],
      epoch: () => 0,
      subscribe(listener) {
        notify = listener;
        return () => {};
      }
    })
  );
  settle(() => notify({ type: "deleted", ids: [2000] }));
  expect(result.ready).toBe(true);
  expect(result.failed).toBe(false);
  expect(result.selected[0]?.id).toBe(1);
});

describe("ranged (spec) selections", () => {
  const TOTAL = 50_000;
  async function setupRanged() {
    api.startAssetQuery.mockResolvedValue({
      status: "ready",
      session_id: 4,
      revision: 1,
      offset: 0,
      total: TOTAL,
      items: [asset(1), asset(2)]
    });
    const view = renderHook(() => {
      const library = createLibrary();
      const selection = createSelection({
        queryEpoch: () => library.query.queryEpoch,
        identityEpoch: () => library.tags.epoch,
        getIdsRange: library.query.getIdsRangeAsync,
        sessionId: library.query.sessionId,
        total: () => library.query.total
      });
      const bulk = createBulk(library, selection, () => false);
      return { library, selection, bulk };
    });
    const { library, selection } = view.result;
    await settle(() => library.refreshLibrary());
    settle(() => selection.toggle());
    await settle(() => selection.interact({ type: "select-all" }));
    settle(() =>
      selection.interact({ type: "click", assetId: 2, assetIndex: 1, ctrlLike: true, shift: false })
    );
    return view.result;
  }
  const ready = (overrides: Record<string, unknown> = {}) => ({
    status: "ready",
    processed_assets: TOTAL - 1,
    updated_assets: TOTAL - 1,
    revision: 5,
    query_impact: { type: "tags", changed_tags: ["sunset"], tag_count_changed: true },
    vocabulary_changed: true,
    is_favorite: null,
    media_group_key: null,
    ...overrides
  });

  it("adds a tag and toggles favorites through one spec write each, without loading summaries", async () => {
    const { bulk } = await setupRanged();
    expect(bulk.selectedCount).toBe(TOTAL - 1);
    expect(bulk.selectionRanged).toBe(true);
    expect(bulk.tagMode).toBe("multiple");
    expect(bulk.metadataLoading).toBe(false);
    expect(bulk.allSelectedFavorites).toBe(false);
    expect(api.getAssetSummariesByIds).not.toHaveBeenCalled();
    const spec = { sessionId: 4, ranges: [[0, TOTAL - 1]], excluded: [2], included: [] };
    api.applySelectionMutation.mockResolvedValueOnce(ready());
    await expect(settle(() => bulk.onAddTag("Sunset"))).resolves.toBe(true);
    expect(api.applySelectionMutation).toHaveBeenCalledExactlyOnceWith(spec, {
      type: "mergeTags",
      tags: ["sunset"]
    });
    expect(api.mergeAssetTagsBulk).not.toHaveBeenCalled();
    expect(bulk.appliedBulkTags).toEqual(["sunset"]);
    expect(bulk.tagSaveFailed).toBe(false);

    api.applySelectionMutation.mockResolvedValueOnce(
      ready({ query_impact: { type: "none" }, vocabulary_changed: false, is_favorite: true })
    );
    await settle(() => bulk.onToggleFavorite());
    expect(api.applySelectionMutation).toHaveBeenLastCalledWith(spec, { type: "toggleFavorite" });
    expect(api.toggleAssetsFavoriteBulk).not.toHaveBeenCalled();
    expect(bulk.favoriteFailed).toBe(false);
  });

  it("reports a stale spec as failed, writes nothing locally and refreshes the query", async () => {
    const { bulk, library } = await setupRanged();
    const starts = api.startAssetQuery.mock.calls.length;
    api.applySelectionMutation.mockResolvedValue({ status: "stale" });
    await expect(settle(() => bulk.onAddTag("late"))).resolves.toBe(false);
    expect(bulk.tagSaveFailed).toBe(true);
    expect(bulk.appliedBulkTags).toEqual([]);
    await settle(() => bulk.onToggleFavorite());
    expect(bulk.favoriteFailed).toBe(true);
    await waitFor(() => expect(api.startAssetQuery.mock.calls.length).toBeGreaterThan(starts));
    expect(library.query.total).toBe(TOTAL);
  });

  it("groups a ranged selection in gallery order through the spec", async () => {
    const { bulk } = await setupRanged();
    api.applySelectionMutation.mockResolvedValueOnce(
      ready({ query_impact: { type: "none" }, media_group_key: "trip" })
    );
    bulk.onGroupKeyDraftChange(" trip ");
    await expect(settle(() => bulk.onApplyGroup())).resolves.toEqual({ status: "saved" });
    expect(api.applySelectionMutation).toHaveBeenCalledWith(
      { sessionId: 4, ranges: [[0, TOTAL - 1]], excluded: [2], included: [] },
      { type: "setMediaGroup", mediaGroupKey: "trip" }
    );
    expect(api.setAssetsMediaGroupBulk).not.toHaveBeenCalled();
    // An explicit order needs materialized IDs, which a range does not have.
    await expect(bulk.onApplyGroup([1, 3])).resolves.toEqual({ status: "ignored" });
  });
});
