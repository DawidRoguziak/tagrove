import type { JSX } from "@solidjs/web";

export function renderHighlightedMatch(
  value: string,
  indices: ReadonlyArray<readonly [number, number]>
): JSX.Element {
  if (!indices.length) {
    return value;
  }

  const sorted = [...indices].sort((a, b) => a[0] - b[0]);
  const fragments: JSX.Element[] = [];
  let cursor = 0;

  for (let index = 0; index < sorted.length; index += 1) {
    const [rawStart, rawEnd] = sorted[index];
    const start = Math.max(rawStart, cursor);
    const end = Math.min(rawEnd + 1, value.length);

    if (start > cursor) {
      fragments.push(<span>{value.slice(cursor, start)}</span>);
    }

    if (end > start) {
      fragments.push(<span class="font-bold text-primary-text">{value.slice(start, end)}</span>);
    }

    cursor = Math.max(cursor, end);
  }

  if (cursor < value.length) {
    fragments.push(<span>{value.slice(cursor)}</span>);
  }

  return fragments;
}
