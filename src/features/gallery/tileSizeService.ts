export const DEFAULT_TILE_SIZE = 188;
export const TILE_SIZE_MIN = 140;
export const TILE_SIZE_MAX = 340;
export const TILE_SIZE_STEP = 8;

export function clampTileSize(nextSize: number): number {
  return Math.max(TILE_SIZE_MIN, Math.min(TILE_SIZE_MAX, nextSize));
}

export function getNextTileSizeByWheel(currentSize: number, deltaY: number): number {
  if (deltaY === 0) {
    return currentSize;
  }

  const step = deltaY > 0 ? -TILE_SIZE_STEP : TILE_SIZE_STEP;
  return clampTileSize(currentSize + step);
}
