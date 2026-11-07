import type { AssetQuerySection } from "../../platform/tauri/contracts/assets";

/** One labelled month of the timeline: chunks of a split month merged back. */
export interface TimelineMonth {
  /** `yyyymm` in UTC; `0` means no date. */
  t: number;
  start: number;
  count: number;
}

/** Merges adjacent sections with the same month (the backend splits large months). */
export function timelineMonths(sections: readonly AssetQuerySection[]): TimelineMonth[] {
  const months: TimelineMonth[] = [];
  for (const section of sections) {
    const last = months[months.length - 1];
    if (last && last.t === section.t && last.start + last.count === section.start)
      last.count += section.count;
    else months.push({ t: section.t, start: section.start, count: section.count });
  }
  return months;
}

/** Index of the month that contains `index` (binary search; months are in index order). */
export function monthAt(months: readonly TimelineMonth[], index: number): number {
  let low = 0;
  let high = months.length - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (months[middle].start <= index) low = middle;
    else high = middle - 1;
  }
  return low;
}

/** Global index for a track fraction: position in the timeline is `index / total`. */
export function indexAtFraction(fraction: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(total - 1, Math.max(0, Math.floor(Math.min(1, Math.max(0, fraction)) * total)));
}

export const yearOf = (t: number) => Math.floor(t / 100);

export type TimelineKey = "ArrowDown" | "ArrowUp" | "PageDown" | "PageUp" | "Home" | "End";

/**
 * Target index for a timeline key from the first visible index. Index order runs from
 * newest to oldest: ArrowDown/ArrowUp step one month, PageDown/PageUp one year (the first
 * month of the next or current/previous year), Home/End the ends. Moving up first returns to
 * the start of the current month or year when the view is inside it. `null` means no move.
 */
export function timelineKeyTarget(
  key: TimelineKey,
  months: readonly TimelineMonth[],
  firstVisible: number,
  total: number
): number | null {
  if (!months.length || total <= 0) return null;
  const current = monthAt(months, firstVisible);
  const yearStart = (month: number) => {
    let first = month;
    while (first > 0 && yearOf(months[first - 1].t) === yearOf(months[month].t)) first--;
    return first;
  };
  switch (key) {
    case "Home":
      return 0;
    case "End":
      return months[months.length - 1].start;
    case "ArrowDown":
      return current + 1 < months.length ? months[current + 1].start : null;
    case "ArrowUp":
      if (firstVisible > months[current].start) return months[current].start;
      return current > 0 ? months[current - 1].start : null;
    case "PageDown": {
      let next = current + 1;
      while (next < months.length && yearOf(months[next].t) === yearOf(months[current].t)) next++;
      return next < months.length ? months[next].start : null;
    }
    case "PageUp": {
      const first = yearStart(current);
      if (firstVisible > months[first].start) return months[first].start;
      return first > 0 ? months[yearStart(first - 1)].start : null;
    }
  }
}

/** Localized month label; the backend's months are UTC calendar months. */
export function formatTimelineMonth(t: number, language: string, noDate: string): string {
  if (t <= 0) return noDate;
  const date = new Date(Date.UTC(yearOf(t), (t % 100) - 1, 1));
  try {
    return new Intl.DateTimeFormat(language, { year: "numeric", month: "long", timeZone: "UTC" }).format(
      date
    );
  } catch {
    return `${yearOf(t)}-${String(t % 100).padStart(2, "0")}`;
  }
}
