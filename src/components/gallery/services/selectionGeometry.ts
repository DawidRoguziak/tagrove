import type { SelectionRange } from "../selection";

export interface Point {
  x: number;
  y: number;
}
export interface SelectionRectangle {
  left: number;
  top: number;
  width: number;
  height: number;
}
export interface GridGeometry {
  columnCount: number;
  tilePixelSize: number;
  tileGap: number;
  assetCount: number;
}

export function rectangleBetween(start: Point, end: Point): SelectionRectangle {
  return {
    left: Math.min(start.x, end.x),
    top: Math.min(start.y, end.y),
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y)
  };
}

export function intersectedRanges(rect: SelectionRectangle, grid: GridGeometry): SelectionRange[] {
  const { columnCount, tilePixelSize, tileGap, assetCount } = grid;
  if (!rect.width || !rect.height || !assetCount) return [];
  const stride = tilePixelSize + tileGap;
  const firstColumn = Math.max(0, Math.floor((rect.left - tilePixelSize) / stride) + 1);
  const lastColumn = Math.min(columnCount - 1, Math.ceil((rect.left + rect.width) / stride) - 1);
  const firstRow = Math.max(0, Math.floor((rect.top - tilePixelSize) / stride) + 1);
  const lastRow = Math.min(
    Math.ceil(assetCount / columnCount) - 1,
    Math.ceil((rect.top + rect.height) / stride) - 1
  );
  if (firstColumn > lastColumn) return [];
  const ranges: SelectionRange[] = [];
  for (let row = firstRow; row <= lastRow; row++) {
    const startIndex = row * columnCount + firstColumn;
    const endIndex = Math.min(assetCount - 1, row * columnCount + lastColumn);
    if (startIndex > endIndex) continue;
    const previous = ranges[ranges.length - 1];
    if (previous && previous.endIndex + 1 === startIndex) previous.endIndex = endIndex;
    else ranges.push({ startIndex, endIndex });
  }
  return ranges;
}

export function containsIndex(ranges: SelectionRange[], index: number): boolean {
  let low = 0;
  let high = ranges.length - 1;
  while (low <= high) {
    const mid = (low + high) >>> 1;
    const range = ranges[mid];
    if (index < range.startIndex) high = mid - 1;
    else if (index > range.endIndex) low = mid + 1;
    else return true;
  }
  return false;
}

export function edgeScrollSpeed(y: number, top: number, bottom: number): number {
  const edge = Math.min(48, (bottom - top) / 2);
  if (y < top + edge) return -900 * Math.min(1, (top + edge - y) / edge);
  if (y > bottom - edge) return 900 * Math.min(1, (y - bottom + edge) / edge);
  return 0;
}
