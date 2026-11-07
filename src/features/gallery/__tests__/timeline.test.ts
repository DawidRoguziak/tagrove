import { describe, expect, it } from "vitest";
import {
  formatTimelineMonth,
  indexAtFraction,
  monthAt,
  timelineKeyTarget,
  timelineMonths
} from "../timeline";

// Newest first, as the backend orders them; 2024-03 was split into two chunks.
const sections = [
  { t: 202403, start: 0, count: 5000 },
  { t: 202403, start: 5000, count: 100 },
  { t: 202402, start: 5100, count: 50 },
  { t: 202312, start: 5150, count: 20 },
  { t: 202306, start: 5170, count: 30 },
  { t: 202201, start: 5200, count: 10 },
  { t: 0, start: 5210, count: 5 }
];
const total = 5215;
const months = timelineMonths(sections);

describe("timeline", () => {
  it("merges split months back into one labelled month", () => {
    expect(months.map((month) => [month.t, month.start, month.count])).toEqual([
      [202403, 0, 5100],
      [202402, 5100, 50],
      [202312, 5150, 20],
      [202306, 5170, 30],
      [202201, 5200, 10],
      [0, 5210, 5]
    ]);
    expect(timelineMonths([])).toEqual([]);
  });

  it("finds the month of an index and maps track fractions to indexes", () => {
    expect(monthAt(months, 0)).toBe(0);
    expect(monthAt(months, 5099)).toBe(0);
    expect(monthAt(months, 5100)).toBe(1);
    expect(monthAt(months, 5214)).toBe(5);
    expect(indexAtFraction(-1, total)).toBe(0);
    expect(indexAtFraction(0.5, total)).toBe(2607);
    expect(indexAtFraction(1, total)).toBe(total - 1);
    expect(indexAtFraction(0.5, 0)).toBe(0);
    // A million-row index is exact without materializing anything.
    expect(indexAtFraction(0.999999, 1_000_000)).toBe(999_999);
  });

  it("steps months with arrows, years with PageUp/PageDown and ends with Home/End", () => {
    const key = (name: Parameters<typeof timelineKeyTarget>[0], from: number) =>
      timelineKeyTarget(name, months, from, total);
    expect(key("ArrowDown", 0)).toBe(5100);
    expect(key("ArrowDown", 5210)).toBeNull();
    // Inside a month, up returns to its start first.
    expect(key("ArrowUp", 5120)).toBe(5100);
    expect(key("ArrowUp", 5100)).toBe(0);
    expect(key("ArrowUp", 0)).toBeNull();
    expect(key("PageDown", 0)).toBe(5150);
    expect(key("PageDown", 5150)).toBe(5200);
    expect(key("PageDown", 5200)).toBe(5210);
    expect(key("PageUp", 5175)).toBe(5150);
    expect(key("PageUp", 5150)).toBe(0);
    expect(key("PageUp", 5210)).toBe(5200);
    expect(key("Home", 5000)).toBe(0);
    expect(key("End", 0)).toBe(5210);
    expect(timelineKeyTarget("End", [], 0, 0)).toBeNull();
  });

  it("formats UTC months in the active language and labels missing dates", () => {
    expect(formatTimelineMonth(202403, "en", "No date")).toBe("March 2024");
    expect(formatTimelineMonth(202401, "de", "Ohne Datum")).toBe("Januar 2024");
    expect(formatTimelineMonth(0, "pl", "Bez daty")).toBe("Bez daty");
  });
});
