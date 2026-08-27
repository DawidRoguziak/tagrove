import { describe, expect, it } from "vitest";
import {
  getNativeVideoFrameViewport,
  getNativeVideoPlayerSize,
  measureNativeVideoBounds,
  NATIVE_VIDEO_CONTROL_STRIP_HEIGHT
} from "../nativeVideoLayout";

describe("nativeVideoLayout", () => {
  it("reserves the control strip while keeping the player inside the viewport", () => {
    const viewport = { width: 1280, height: 800 };
    const frameViewport = getNativeVideoFrameViewport(viewport);
    const playerSize = getNativeVideoPlayerSize({ width: 1280, height: 720 });

    expect(frameViewport).toEqual({
      width: 1280,
      height: 800 - NATIVE_VIDEO_CONTROL_STRIP_HEIGHT
    });
    expect(playerSize).toEqual({
      width: 1280,
      height: 720 + NATIVE_VIDEO_CONTROL_STRIP_HEIGHT
    });
    expect(playerSize.height).toBeLessThanOrEqual(viewport.height);
  });

  it("publishes rounded native bounds above the control strip", () => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () => ({
      x: 12.4,
      y: 24.6,
      width: 640.4,
      height: 432.2,
      top: 24.6,
      left: 12.4,
      right: 652.8,
      bottom: 456.8,
      toJSON: () => ({})
    });

    expect(measureNativeVideoBounds(element)).toEqual({
      x: 12,
      y: 25,
      width: 641,
      height: 360
    });
  });

  it("does not publish a negative frame height for a short container", () => {
    expect(getNativeVideoFrameViewport({ width: 200, height: 40 })).toEqual({
      width: 200,
      height: 0
    });
    expect(getNativeVideoPlayerSize({ width: 0, height: 0 })).toEqual({
      width: 0,
      height: 0
    });
  });
});
