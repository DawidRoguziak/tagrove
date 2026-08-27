import { describe, expect, it } from "vitest";
import { measureNativeVideoBounds } from "../nativeVideoLayout";

describe("nativeVideoLayout", () => {
  it("uses the complete player footprint for the native frame", () => {
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
      height: 432
    });
  });

  it("keeps a short player height intact", () => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () => ({
      x: 0,
      y: 0,
      width: 200,
      height: 40,
      top: 0,
      left: 0,
      right: 200,
      bottom: 40,
      toJSON: () => ({})
    });

    expect(measureNativeVideoBounds(element)).toEqual({
      x: 0,
      y: 0,
      width: 200,
      height: 40
    });
  });
});
