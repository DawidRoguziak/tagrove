import { describe, expect, it } from "vitest";
import { MAX_GRID_HEIGHT, scrollCoordinates } from "../scrollCoordinates";

describe("gallery scroll coordinates", () => {
  it.each([
    0,
    400,
    MAX_GRID_HEIGHT,
    40_000_000,
    600_000_000
  ])("maps the viewport and footer continuously with total %i", (total) => {
    const map = scrollCoordinates(total, 600, 80);
    expect(map.height).toBe(Math.min(total, MAX_GRID_HEIGHT));
    expect(map.toLogical(0)).toBe(0);
    expect(map.toLogical(80)).toBe(80);
    for (const physical of [40, 80, 2000, map.height / 2, map.height - 520, map.height - 384]) {
      expect(map.toPhysical(map.toLogical(physical))).toBeCloseTo(physical, 5);
    }
    if (map.compressed) {
      expect(map.toLogical(map.height - 520)).toBeCloseTo(total - 520);
      expect(map.toLogical(map.height - 384)).toBeCloseTo(total - 384);
    } else expect(map.toLogical(2000)).toBe(2000);
  });
});
