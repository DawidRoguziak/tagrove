import type { LibraryRange } from "../../library/createLibrary";

/**
 * Tallest physical grid spacer. WebKitGTK 2.54 keeps layout, scroll offsets, transforms,
 * hit testing and painting exact only below 2^24 CSS px (16,777,216) at device scales 1
 * and 2; above it odd offsets round and transformed rows stop painting. The remaining
 * ~0.78M px cover the gallery margin, footer and the stacked bulk inspector.
 * Measured by `e2e/gallery-scale/height-probe.tsx` (stage 05 gate).
 */
export const MAX_GRID_HEIGHT = 16_000_000;
export const GRID_GAP = 12;
export const OVERSCAN_ROWS = 4;

export interface GridLayout {
  columns: number;
  size: number;
  gap: number;
  stride: number;
  count: number;
  rows: number;
  contentHeight: number;
}
export interface RowRange {
  first: number;
  last: number;
  visibleFirst: number;
  visibleLast: number;
}
export const EMPTY_ROWS: RowRange = { first: 0, last: -1, visibleFirst: 0, visibleLast: -1 };

/** Square tiles fill the width: as many columns as fit at `tileMin`, then stretched. */
export function gridLayout(input: {
  width: number;
  tileMin: number;
  gap: number;
  count: number;
}): GridLayout {
  const { width, tileMin, gap } = input;
  const count = Math.max(0, Math.floor(input.count));
  const columns = Math.max(1, Math.floor((width + gap) / (tileMin + gap)));
  const size = Math.max(1, Math.floor((Math.max(width, tileMin) - gap * (columns - 1)) / columns));
  const stride = size + gap;
  const rows = Math.ceil(count / columns);
  return { columns, size, gap, stride, count, rows, contentHeight: rows ? rows * stride - gap : 0 };
}

export const rowTop = (layout: GridLayout, row: number) => row * layout.stride;
export const indexToRow = (layout: GridLayout, index: number) => Math.floor(index / layout.columns);

/**
 * Rows intersecting `[offset, offset + viewport)` in grid coordinates, plus overscan.
 * `offset` may be negative while the gallery margin above the grid is visible.
 */
export function rowRange(
  layout: GridLayout,
  offset: number,
  viewport: number,
  overscan = OVERSCAN_ROWS
): RowRange {
  if (!layout.rows) return EMPTY_ROWS;
  const last = layout.rows - 1;
  const visibleFirst = Math.min(last, Math.max(0, Math.floor(offset / layout.stride)));
  const visibleLast = Math.min(
    last,
    Math.max(visibleFirst, Math.ceil((offset + Math.max(0, viewport)) / layout.stride) - 1)
  );
  return {
    first: Math.max(0, visibleFirst - overscan),
    last: Math.min(last, visibleLast + overscan),
    visibleFirst,
    visibleLast
  };
}

/** Inclusive global indices for page and thumbnail demand. */
export function toLibraryRange(layout: GridLayout, rows: RowRange): LibraryRange {
  if (rows.last < rows.first) return { startIndex: 0, endIndex: -1, visibleStartIndex: 0, visibleEndIndex: -1 };
  const end = (row: number) => Math.min(layout.count - 1, (row + 1) * layout.columns - 1);
  return {
    startIndex: rows.first * layout.columns,
    endIndex: end(rows.last),
    visibleStartIndex: rows.visibleFirst * layout.columns,
    visibleEndIndex: end(rows.visibleLast)
  };
}

/** Rendered row slots: every row a viewport can intersect plus overscan on both sides. */
export function poolRows(layout: GridLayout, viewport: number, overscan = OVERSCAN_ROWS) {
  return Math.min(layout.rows, Math.ceil(Math.max(0, viewport) / layout.stride) + 1 + 2 * overscan);
}

/**
 * Offset that keeps the previous first visible global index at the top when columns or
 * tile size change. Offsets inside the margin above the grid are kept as they are.
 */
export function anchorForResize(previous: GridLayout, next: GridLayout, offset: number) {
  if (offset <= 0 || (previous.columns === next.columns && previous.size === next.size))
    return offset;
  const firstIndex = Math.floor(offset / previous.stride) * previous.columns;
  return rowTop(next, indexToRow(next, firstIndex));
}

/** Physical spacer height; content beyond it is reached by moving the window base. */
export const windowHeight = (contentHeight: number) => Math.min(contentHeight, MAX_GRID_HEIGHT);
export const maxBase = (contentHeight: number) => Math.max(0, contentHeight - MAX_GRID_HEIGHT);
export const clampBase = (base: number, contentHeight: number) =>
  Math.min(maxBase(contentHeight), Math.max(0, Math.round(base)));

/** Base that centres a logical viewport in the window, leaving native scroll room both ways. */
export function centeredBase(offset: number, viewport: number, contentHeight: number) {
  return clampBase(offset - (MAX_GRID_HEIGHT - viewport) / 2, contentHeight);
}

/**
 * Whether the physical position `offset - base` is within `band` px of a window edge that
 * the base could still move past. Callers use a wide band when idle and a narrow one
 * (a few viewports) to rebase immediately during continuous motion.
 */
export function needsRebase(
  offset: number,
  viewport: number,
  base: number,
  contentHeight: number,
  band: number
) {
  const position = offset - base;
  return (
    (base > 0 && position < band) ||
    (base < maxBase(contentHeight) && position + viewport > windowHeight(contentHeight) - band)
  );
}
