import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  beginVideoOpen,
  cancelVideoOpen,
  setVideoControlLabels,
  closeVideo,
  controlVideo,
  listAssets,
  mergeAssetTagsBulk,
  openVideo,
  setAssetTags,
  setVideoBounds,
  toMediaSrc,
  toggleAssetsFavoriteBulk
} from "../api";

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
  it("sends bulk favorite IDs and returns the canonical toggle result", async () => {
    const summary = { processed_asset_ids: [1, 2], is_favorite: true, revision: 3 };
    coreMocks.invoke.mockResolvedValueOnce(summary);
    await expect(toggleAssetsFavoriteBulk([1, 2])).resolves.toEqual(summary);
    expect(coreMocks.invoke).toHaveBeenCalledWith("toggle_assets_favorite_bulk", { assetIds: [1, 2] });
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

  it("maps tag mutation payloads and returns canonical backend summaries", async () => {
    const single = { asset_id: 7, changed: true, tags: ["cat"], revision: 4 };
    const bulk = {
      processed_assets: 1,
      updated_assets: 1,
      processed_asset_ids: [7],
      updated_asset_ids: [7],
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
    coreMocks.invoke.mockResolvedValueOnce(21);
    await expect(beginVideoOpen()).resolves.toBe(21);
    await cancelVideoOpen(21);
    await setVideoControlLabels(17, controlLabels);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(5, "begin_video_open");
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(6, "cancel_video_open", { requestId: 21 });
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(7, "set_video_control_labels", { sessionId: 17, controlLabels });
  });

});
