import { describe, expect, it } from "vitest";
import { containsIndex, intersectedRanges, rectangleBetween } from "../selectionGeometry";
const grid = { columnCount: 3, tilePixelSize: 100, tileGap: 10, assetCount: 8 };
describe("rectangle geometry", () => {
  it.each([[95, 95, 115, 115], [115, 115, 95, 95], [115, 95, 95, 115], [95, 115, 115, 95]])(
    "includes partial overlaps in every direction %s %s %s %s", (x, y, ex, ey) => {
      expect(intersectedRanges(rectangleBetween({ x, y }, { x: ex, y: ey }), grid))
        .toEqual([{ startIndex: 0, endIndex: 1 }, { startIndex: 3, endIndex: 4 }]);
    });
  it("excludes gaps, edge touches, and unused final-row slots", () => {
    expect(intersectedRanges({ left: 100, top: 0, width: 10, height: 320 }, grid)).toEqual([]);
    expect(intersectedRanges({ left: 0, top: 100, width: 320, height: 10 }, grid)).toEqual([]);
    expect(intersectedRanges({ left: 220, top: 220, width: 100, height: 100 }, grid)).toEqual([]);
    expect(intersectedRanges({ left: -20, top: -20, width: 400, height: 500 }, grid)).toEqual([{ startIndex: 0, endIndex: 7 }]);
  });
  it("finds sparse ranges with binary lookup", () => {
    const ranges = [{ startIndex: 2, endIndex: 3 }, { startIndex: 7, endIndex: 10 }];
    expect([1, 2, 3, 4, 7, 10, 11].map(index => containsIndex(ranges, index))).toEqual([false, true, true, false, true, true, false]);
  });
});
