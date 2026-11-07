import { beforeEach, describe, expect, it, vi } from "vitest";
import { getStartupPopularTags, mergeAssetTagsBulk, setAssetTags } from "../tags";
import { applySelectionMutation, getAssetQuerySections } from "../assets";
import { startAssetQuery, getAssetSummariesByIds, getAssetQueryPosition, getAssetQueryIdRange, decodeAssetIdRange, setAssetsMediaGroupBulk, listAssets, toMediaSrc, toggleAssetsFavoriteBulk } from "../assets";
import { idRangeBuffer } from "../../../test/idRange";
import { thumbnailUrl } from "../thumbnails";
import { listScanRoots, setScanRootAutoScan, scanStartupRoots } from "../scanning";
import { beginVideoOpen, cancelVideoOpen, setVideoControlLabels, closeVideo, controlVideo, openVideo, setVideoBounds } from "../video";

const coreMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `tauri://${path}`),
  channels: [] as Array<{ onmessage?: (event: unknown) => void }>
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: coreMocks.invoke,
  convertFileSrc: coreMocks.convertFileSrc,
  Channel: class {
    onmessage?: (event: unknown) => void;

    constructor() {
      coreMocks.channels.push(this);
    }
  }
}));

describe("api contract", () => {
  it("reads the ordered startup tag snapshot without arguments", async () => {
    coreMocks.invoke.mockResolvedValueOnce(["travel", "cat"]);
    await expect(getStartupPopularTags()).resolves.toEqual(["travel", "cat"]);
    expect(coreMocks.invoke).toHaveBeenCalledWith("get_startup_popular_tags");
  });
  it.each([
    { status: "resolved", index: 201 },
    { status: "missing" },
    { status: "stale" }
  ])("looks up query position through IPC: %j", async (response) => {
    coreMocks.invoke.mockResolvedValueOnce(response);
    await expect(getAssetQueryPosition(7, 202)).resolves.toEqual(response);
    expect(coreMocks.invoke).toHaveBeenCalledWith("get_asset_query_position", {
      sessionId: 7,
      assetId: 202
    });
  });
  it.each([
    {
      status: "ready",
      session_id: 7,
      total: 3,
      sections: [
        { t: 202403, start: 0, count: 2 },
        { t: 0, start: 2, count: 1 }
      ]
    },
    { status: "stale" }
  ])("reads timeline sections of a session: %j", async (response) => {
    coreMocks.invoke.mockResolvedValueOnce(response);
    await expect(getAssetQuerySections(7)).resolves.toEqual(response);
    expect(coreMocks.invoke).toHaveBeenCalledWith("get_asset_query_sections", { sessionId: 7 });
  });
  it("sends a selection spec and mutation with camelCase keys", async () => {
    const ready = {
      status: "ready",
      processed_assets: 3,
      updated_assets: 2,
      revision: 9,
      query_impact: { type: "tags", changed_tags: ["a"], tag_count_changed: true },
      vocabulary_changed: true,
      is_favorite: null,
      media_group_key: null
    };
    coreMocks.invoke.mockResolvedValueOnce(ready).mockResolvedValueOnce({ status: "stale" });
    const selection = { sessionId: 4, ranges: [[0, 9]] as Array<[number, number]>, excluded: [2], included: [50] };
    await expect(applySelectionMutation(selection, { type: "mergeTags", tags: ["a"] })).resolves.toEqual(ready);
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("apply_selection_mutation", {
      selection: { sessionId: 4, ranges: [[0, 9]], excluded: [2], included: [50] },
      mutation: { type: "mergeTags", tags: ["a"] }
    });
    await expect(
      applySelectionMutation(selection, { type: "setMediaGroup", mediaGroupKey: null })
    ).resolves.toEqual({ status: "stale" });
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("apply_selection_mutation", {
      selection: { sessionId: 4, ranges: [[0, 9]], excluded: [2], included: [50] },
      mutation: { type: "setMediaGroup", mediaGroupKey: null }
    });
  });
  it("uses the scan root preference and startup command contracts", async () => {
    const roots = [{ path: "/media", auto_scan_on_startup: true }];
    coreMocks.invoke.mockResolvedValueOnce(roots);
    await expect(listScanRoots()).resolves.toEqual(roots);
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("list_scan_roots");
    await setScanRootAutoScan("/media", true);
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("set_scan_root_auto_scan", {
      path: "/media",
      enabled: true
    });
    coreMocks.invoke.mockResolvedValueOnce(null);
    await expect(scanStartupRoots()).resolves.toBeNull();
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("scan_startup_roots");
  });
  it("preserves shared startup results and failures across repeated observations", async () => {
    const summary = { completion: "complete", indexed: 3, removed: 0, failed: 0 };
    coreMocks.invoke.mockResolvedValue(summary);
    await expect(scanStartupRoots()).resolves.toEqual(summary);
    await expect(scanStartupRoots()).resolves.toEqual(summary);
    coreMocks.invoke.mockRejectedValue("scan failed");
    await expect(scanStartupRoots()).rejects.toBe("scan failed");
    expect(coreMocks.invoke).toHaveBeenCalledTimes(3);
  });
  it("sends bulk favorite IDs and returns the canonical toggle result", async () => {
    const summary = { processed_asset_ids: [1, 2], is_favorite: true, revision: 3 };
    coreMocks.invoke.mockResolvedValueOnce(summary);
    await expect(toggleAssetsFavoriteBulk([1, 2])).resolves.toEqual(summary);
    expect(coreMocks.invoke).toHaveBeenCalledWith("toggle_assets_favorite_bulk", {
      assetIds: [1, 2]
    });
  });

  beforeEach(() => {
    coreMocks.invoke.mockReset();
    coreMocks.convertFileSrc.mockClear();
    coreMocks.channels.length = 0;
  });

  it("maps mediaKind=all to null kind in listAssets invoke payload", async () => {
    coreMocks.invoke.mockResolvedValueOnce({ items: [], total: 0 });

    await listAssets({
      offset: 10,
      limit: 20,
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      mediaKind: "all",
      favoritesOnly: true
    });

    expect(coreMocks.invoke).toHaveBeenCalledWith("list_assets", {
      offset: 10,
      limit: 20,
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      kind: null,
      favoritesOnly: true,
      metaFilter: null
    });
  });

  it.each([
    false,
    true
  ])("maps tag mutation summaries with vocabulary_changed=%s", async (vocabularyChanged) => {
    const single: Awaited<ReturnType<typeof setAssetTags>> = {
      vocabulary_changed: vocabularyChanged,
      query_impact: { type: "tags", changed_tags: ["cat"], tag_count_changed: true },
      asset_id: 7,
      changed: true,
      tags: ["cat"],
      revision: 4
    };
    const bulk: Awaited<ReturnType<typeof mergeAssetTagsBulk>> = {
      processed_assets: 1,
      updated_assets: 1,
      processed_asset_ids: [7],
      updated_asset_ids: [7],
      vocabulary_changed: vocabularyChanged,
      query_impact: { type: "tags", changed_tags: ["travel"], tag_count_changed: true },
      results: [{ asset_id: 7, changed: true, tags: ["cat", "travel"] }],
      revision: 5
    };
    coreMocks.invoke.mockResolvedValueOnce(single).mockResolvedValueOnce(bulk);

    await expect(setAssetTags(7, ["cat"])).resolves.toEqual(single);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, "set_asset_tags", {
      assetId: 7,
      tags: ["cat"]
    });
    await expect(mergeAssetTagsBulk([7, 9], ["travel"])).resolves.toEqual(bulk);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, "merge_asset_tags_bulk", {
      assetIds: [7, 9],
      tags: ["travel"]
    });
  });

  it("converts a media path through Tauri", () => {
    const result = toMediaSrc("/library/cats/photo.jpg");

    expect(coreMocks.convertFileSrc).toHaveBeenCalledWith("/library/cats/photo.jpg");
    expect(result).toBe("tauri:///library/cats/photo.jpg");
  });

  it("maps native video session payloads and channel events", async () => {
    const onEvent = vi.fn();
    const bounds = { x: 10, y: 20, width: 640, height: 360 };
    const controlLabels = {
      play: "Play",
      pause: "Pause",
      mute: "Mute",
      unmute: "Unmute",
      volume: "Volume",
      seek: "Seek",
      playbackRate: "Playback speed",
      fullscreen: "Fullscreen",
      exitFullscreen: "Exit fullscreen"
    };
    coreMocks.invoke.mockResolvedValueOnce(17).mockResolvedValue(undefined);

    await expect(openVideo(4, 9, bounds, controlLabels, onEvent)).resolves.toBe(17);
    const channel = coreMocks.channels[0];
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, "open_video", {
      assetId: 4,
      requestId: 9,
      bounds,
      controlLabels,
      onEvent: channel
    });
    channel?.onmessage?.({ session_id: 17, type: "loading" });
    expect(onEvent).toHaveBeenCalledWith({ session_id: 17, type: "loading" });

    channel?.onmessage?.({ session_id: 17, type: "escape" });
    expect(onEvent).toHaveBeenLastCalledWith({ session_id: 17, type: "escape" });

    await setVideoBounds(17, bounds);
    await controlVideo(17, { type: "seek", time: 3.5 });
    await closeVideo(17);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, "set_video_bounds", {
      sessionId: 17,
      bounds
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(3, "control_video", {
      sessionId: 17,
      command: { type: "seek", time: 3.5 }
    });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(4, "close_video", { sessionId: 17 });
    await controlVideo(17, { type: "togglePause" });
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("control_video", {
      sessionId: 17,
      command: { type: "togglePause" }
    });
    await controlVideo(17, { type: "seekRelative", seconds: -5 });
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("control_video", {
      sessionId: 17,
      command: { type: "seekRelative", seconds: -5 }
    });
    coreMocks.invoke.mockResolvedValueOnce(21);
    await expect(beginVideoOpen()).resolves.toBe(21);
    await cancelVideoOpen(21);
    await setVideoControlLabels(17, controlLabels);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(7, "begin_video_open");
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(8, "cancel_video_open", { requestId: 21 });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(9, "set_video_control_labels", {
      sessionId: 17,
      controlLabels
    });
  });

  it("uses the selection summary and processed group ID contracts", async () => {
    coreMocks.invoke.mockResolvedValueOnce([]);
    await expect(getAssetSummariesByIds([3, 1, 999])).resolves.toEqual([]);
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("get_asset_summaries_by_ids", {
      assetIds: [3, 1, 999]
    });
    const result = {
      processed_assets: 1,
      processed_asset_ids: [3],
      updated_assets: 1,
      media_group_key: "trip"
    };
    coreMocks.invoke.mockResolvedValueOnce(result);
    await expect(
      setAssetsMediaGroupBulk([{ assetId: 3, mediaGroupOrder: 1 }], "trip")
    ).resolves.toEqual(result);
  });

  it("sends query owner and generation and preserves capacity rejection", async () => {
    coreMocks.invoke.mockRejectedValueOnce("Query exceeds the retained session memory limit. Narrow your search and try again.");
    await expect(startAssetQuery({ tagsAnd: ["small"], tagsNot: [], mediaKind: "all", favoritesOnly: false, ownerId: "owner-test", generation: 2 })).rejects.toContain("Narrow your search");
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("start_asset_query", {
      tagsAnd: ["small"], tagsNot: [], kind: null, favoritesOnly: false, metaFilter: null,
      ownerId: "owner-test", generation: 2, pageSize: 128, anchorOffset: null
    });
  });

  it("sends the refresh anchor offset", async () => {
    coreMocks.invoke.mockResolvedValueOnce({ status: "superseded" });
    await startAssetQuery({ tagsAnd: [], tagsNot: [], mediaKind: "image", favoritesOnly: true, ownerId: "owner-test", generation: 3, pageSize: 64, anchorOffset: 5000 });
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("start_asset_query", {
      tagsAnd: [], tagsNot: [], kind: "image", favoritesOnly: true, metaFilter: null,
      ownerId: "owner-test", generation: 3, pageSize: 64, anchorOffset: 5000
    });
  });

  it("decodes a binary ID range from little-endian f64 values", async () => {
    const body = new ArrayBuffer(32);
    const view = new DataView(body);
    [3, 7, 2 ** 53, 1].forEach((value, index) => view.setFloat64(index * 8, value, true));
    coreMocks.invoke.mockResolvedValueOnce(body);
    const result = await getAssetQueryIdRange(4, 10, 12);
    expect(coreMocks.invoke).toHaveBeenLastCalledWith("get_asset_query_id_range", {
      sessionId: 4,
      start: 10,
      end: 12
    });
    if (result.status !== "ready") throw new Error("expected ready");
    expect(Array.from(result.ids)).toEqual([7, 2 ** 53, 1]);
  });

  it("round-trips the shared binary test body through invoke", async () => {
    coreMocks.invoke.mockResolvedValueOnce(idRangeBuffer([4, 5, 6]));
    const result = await getAssetQueryIdRange(1, 3, 5);
    expect(result.status === "ready" && Array.from(result.ids)).toEqual([4, 5, 6]);
    coreMocks.invoke.mockResolvedValueOnce(new ArrayBuffer(0));
    await expect(getAssetQueryIdRange(1, 3, 5)).resolves.toEqual({ status: "stale" });
  });

  it("decodes stale, empty, unaligned and byte-array ID ranges and rejects malformed ones", () => {
    expect(decodeAssetIdRange(new ArrayBuffer(0))).toEqual({ status: "stale" });
    expect(decodeAssetIdRange([])).toEqual({ status: "stale" });
    const empty = decodeAssetIdRange(new Float64Array([0]).buffer);
    expect(empty.status === "ready" && empty.ids.length).toBe(0);
    const padded = new Uint8Array(25);
    padded.set(new Uint8Array(new Float64Array([2, 9, 11]).buffer), 1);
    const unaligned = decodeAssetIdRange(padded.subarray(1));
    expect(unaligned.status === "ready" && Array.from(unaligned.ids)).toEqual([9, 11]);
    const plain = decodeAssetIdRange(Array.from(new Uint8Array(new Float64Array([1, 5]).buffer)));
    expect(plain.status === "ready" && Array.from(plain.ids)).toEqual([5]);
    expect(() => decodeAssetIdRange(new ArrayBuffer(12))).toThrow("malformed");
    expect(() => decodeAssetIdRange(new Float64Array([3, 1]).buffer)).toThrow("incomplete");
    expect(() => decodeAssetIdRange({})).toThrow("not binary");
  });
  it("builds thumb:// URLs from the asset ID and version token only", () => {
    expect(thumbnailUrl(42, "0123456789abcdef")).toBe("thumb://localhost/42?v=0123456789abcdef");
    expect(thumbnailUrl(42, "0123456789abcdef", { cachedOnly: true })).toBe(
      "thumb://localhost/42?v=0123456789abcdef&c=1"
    );
    expect(thumbnailUrl(42, "0123456789abcdef", { retry: 2 })).toBe(
      "thumb://localhost/42?v=0123456789abcdef&r=2"
    );
    expect(thumbnailUrl(42, "0123456789abcdef", { prefetch: true, retry: 1 })).toBe(
      "thumb://localhost/42?v=0123456789abcdef&p=1&r=1"
    );
  });
});
