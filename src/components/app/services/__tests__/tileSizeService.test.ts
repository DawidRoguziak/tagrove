import { describe, expect, it } from "vitest";
import {
  TILE_SIZE_MAX,
  TILE_SIZE_MIN,
  TILE_SIZE_STEP,
  clampTileSize,
  getNextTileSizeByWheel
} from "../tileSizeService";

describe("tileSizeService", () => {
  it("clamps tile size to allowed range", () => {
    expect(clampTileSize(TILE_SIZE_MIN - 100)).toBe(TILE_SIZE_MIN);
    expect(clampTileSize(TILE_SIZE_MAX + 100)).toBe(TILE_SIZE_MAX);
    expect(clampTileSize(200)).toBe(200);
  });

  it("adjusts tile size by wheel direction", () => {
    expect(getNextTileSizeByWheel(200, -1)).toBe(200 + TILE_SIZE_STEP);
    expect(getNextTileSizeByWheel(200, 1)).toBe(200 - TILE_SIZE_STEP);
    expect(getNextTileSizeByWheel(200, 0)).toBe(200);
  });
});
