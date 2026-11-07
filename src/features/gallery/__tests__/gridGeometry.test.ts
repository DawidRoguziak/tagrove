import { describe, expect, it } from "vitest";
import {
  anchorForResize,
  centeredBase,
  clampBase,
  EMPTY_ROWS,
  gridLayout,
  GRID_GAP,
  MAX_GRID_HEIGHT,
  maxBase,
  needsRebase,
  poolRows,
  rowRange,
  rowTop,
  toLibraryRange,
  windowHeight
} from "../gridGeometry";

const layout = (count: number, width = 988, tileMin = 188) =>
  gridLayout({ width, tileMin, gap: GRID_GAP, count });

describe("gridLayout", () => {
  it.each([
    [10_000, 2_000],
    [26_000, 5_200],
    [1_000_000, 200_000]
  ])("lays out %i slots in exact square rows", (count, rows) => {
    const grid = layout(count);
    expect(grid).toMatchObject({ columns: 5, size: 188, stride: 200, rows });
    expect(grid.contentHeight).toBe(rows * 200 - 12);
  });

  it("stretches tiles to the width and keeps one column for narrow widths", () => {
    expect(layout(10, 1000)).toMatchObject({ columns: 5, size: 190 });
    expect(layout(10, 100)).toMatchObject({ columns: 1, size: 188 });
    expect(layout(1_000_000, 588, 340)).toMatchObject({ columns: 1, size: 588, rows: 1_000_000 });
    expect(layout(1_000_000, 588, 340).contentHeight).toBe(1_000_000 * 600 - 12);
  });

  it("keeps a partial last row and no content when empty", () => {
    expect(layout(12)).toMatchObject({ rows: 3, contentHeight: 588 });
    expect(layout(0)).toMatchObject({ rows: 0, contentHeight: 0 });
  });
});

describe("rowRange", () => {
  it("returns no rows for an empty grid", () => {
    expect(rowRange(layout(0), 0, 600)).toEqual(EMPTY_ROWS);
    expect(toLibraryRange(layout(0), EMPTY_ROWS)).toEqual({
      startIndex: 0,
      endIndex: -1,
      visibleStartIndex: 0,
      visibleEndIndex: -1
    });
  });

  it("clips overscan at the start and keeps the margin above the grid at row zero", () => {
    expect(rowRange(layout(10_000), -80, 600)).toEqual({
      first: 0,
      last: 6,
      visibleFirst: 0,
      visibleLast: 2
    });
    expect(rowRange(layout(10_000), 0, 1000)).toMatchObject({ visibleFirst: 0, visibleLast: 4 });
    expect(rowRange(layout(10_000), 0, 1001)).toMatchObject({ visibleFirst: 0, visibleLast: 5 });
  });

  it("maps a middle offset to global indices with four overscan rows", () => {
    const grid = layout(26_000);
    const rows = rowRange(grid, 20_000, 600);
    expect(rows).toEqual({ first: 96, last: 106, visibleFirst: 100, visibleLast: 102 });
    expect(toLibraryRange(grid, rows)).toEqual({
      startIndex: 480,
      endIndex: 534,
      visibleStartIndex: 500,
      visibleEndIndex: 514
    });
  });

  it("clamps the final partial row and an offset beyond the end at the millionth slot", () => {
    const grid = layout(999_998);
    const rows = rowRange(grid, grid.contentHeight - 600, 600);
    expect(rows.last).toBe(grid.rows - 1);
    expect(toLibraryRange(grid, rows)).toMatchObject({ endIndex: 999_997, visibleEndIndex: 999_997 });
    expect(rowRange(grid, 1e12, 600)).toMatchObject({
      visibleFirst: grid.rows - 1,
      visibleLast: grid.rows - 1,
      last: grid.rows - 1
    });
  });

  it("sizes the row pool for any viewport alignment", () => {
    const grid = layout(10_000);
    expect(poolRows(grid, 1000)).toBe(5 + 1 + 8);
    expect(poolRows(layout(12), 1000)).toBe(3);
    for (let offset = 0; offset < 400; offset += 37) {
      const rows = rowRange(grid, 20_000 + offset, 1000);
      expect(rows.last - rows.first + 1).toBeLessThanOrEqual(poolRows(grid, 1000));
    }
  });
});

describe("anchorForResize", () => {
  it("keeps the first visible global index when columns or tile size change", () => {
    const before = layout(10_000);
    const after = layout(10_000, 588);
    expect(after.columns).toBe(3);
    // Row 10 at five columns starts with index 50, which is row 16 at three columns.
    expect(anchorForResize(before, after, 2_000)).toBe(rowTop(after, 16));
    expect(anchorForResize(before, layout(10_000, 988, 340), 2_000)).toBe(
      rowTop(layout(10_000, 988, 340), 25)
    );
  });

  it("keeps unchanged layouts and the margin above the grid", () => {
    const grid = layout(10_000);
    expect(anchorForResize(grid, layout(20_000), 2_345)).toBe(2_345);
    expect(anchorForResize(grid, layout(10_000, 588), -40)).toBe(-40);
  });

  it("anchors at a million slots without precision loss", () => {
    const before = layout(1_000_000);
    const after = layout(1_000_000, 588, 340);
    const offset = rowTop(before, 123_457) + 99;
    expect(anchorForResize(before, after, offset)).toBe(rowTop(after, 123_457 * 5));
  });
});

describe("window base", () => {
  it("keeps native grids at base zero", () => {
    const content = layout(26_000).contentHeight;
    expect(windowHeight(content)).toBe(content);
    expect(maxBase(content)).toBe(0);
    expect(clampBase(5_000, content)).toBe(0);
    expect(needsRebase(content - 600, 600, 0, content, 1_200)).toBe(false);
  });

  it("bounds a one-column million-slot window below the measured WebKit limit", () => {
    const content = layout(1_000_000, 588, 340).contentHeight;
    expect(windowHeight(content)).toBe(MAX_GRID_HEIGHT);
    expect(maxBase(content)).toBe(content - MAX_GRID_HEIGHT);
    expect(clampBase(-5, content)).toBe(0);
    expect(clampBase(1e15, content)).toBe(content - MAX_GRID_HEIGHT);
    const middle = 300_000_000;
    const base = centeredBase(middle, 600, content);
    expect(middle - base).toBe((MAX_GRID_HEIGHT - 600) / 2);
    expect(centeredBase(0, 600, content)).toBe(0);
    expect(centeredBase(content - 600, 600, content)).toBe(content - MAX_GRID_HEIGHT);
  });

  it("asks for a rebase only near an edge the base can still move past", () => {
    const content = layout(1_000_000).contentHeight;
    const quarter = MAX_GRID_HEIGHT / 4;
    expect(needsRebase(100, 600, 0, content, quarter)).toBe(false);
    expect(needsRebase(4_500_000, 600, 1_000_000, content, quarter)).toBe(true);
    expect(needsRebase(9_000_000, 600, 1_000_000, content, quarter)).toBe(false);
    expect(needsRebase(13_000_000, 600, 0, content, quarter)).toBe(true);
    expect(needsRebase(content - 600, 600, maxBase(content), content, quarter)).toBe(false);
    expect(needsRebase(1_002_000, 600, 1_000_000, content, 1_200)).toBe(false);
    expect(needsRebase(1_001_000, 600, 1_000_000, content, 1_200)).toBe(true);
  });
});
