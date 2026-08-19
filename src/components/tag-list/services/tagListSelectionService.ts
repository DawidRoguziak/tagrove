import type { TagListPage } from "../../../types";

export type TagSelectionMode = "include" | "exclude";

export interface TagSelections {
  included: Record<string, string>;
  excluded: Record<string, string>;
}

export function normalizeTagName(tag: string): string {
  return tag.trim().toLowerCase();
}

export function sortTagsCaseInsensitive(tags: string[]): string[] {
  return [...tags].sort((left, right) =>
    left.localeCompare(right, undefined, { sensitivity: "base" })
  );
}

export function dedupeKnownTags(tags: string[]): string[] {
  const unique = new Map<string, string>();

  for (const rawTag of tags) {
    const trimmedTag = rawTag.trim();
    const normalizedTag = normalizeTagName(trimmedTag);
    if (!normalizedTag || unique.has(normalizedTag)) {
      continue;
    }

    unique.set(normalizedTag, trimmedTag);
  }

  return sortTagsCaseInsensitive(Array.from(unique.values()));
}

export function createFallbackPage(
  knownTags: string[],
  query: string,
  offset: number,
  limit: number
): TagListPage {
  const normalizedQuery = normalizeTagName(query);
  const filteredItems = normalizedQuery
    ? knownTags.filter((tag) => normalizeTagName(tag).includes(normalizedQuery))
    : knownTags;

  return {
    items: filteredItems.slice(offset, offset + limit),
    total: filteredItems.length
  };
}

export function mergeTagPages(previousItems: string[], nextItems: string[]): string[] {
  const mergedItems = [...previousItems];
  const known = new Set(previousItems.map((item) => normalizeTagName(item)));

  for (const item of nextItems) {
    const normalizedItem = normalizeTagName(item);
    if (!normalizedItem || known.has(normalizedItem)) {
      continue;
    }

    mergedItems.push(item);
    known.add(normalizedItem);
  }

  return mergedItems;
}

export function getTagMode(selections: TagSelections, tag: string): TagSelectionMode | null {
  const normalizedTag = normalizeTagName(tag);
  if (!normalizedTag) {
    return null;
  }

  if (selections.included[normalizedTag]) {
    return "include";
  }

  if (selections.excluded[normalizedTag]) {
    return "exclude";
  }

  return null;
}

export function removeTagSelection(selections: TagSelections, tag: string): TagSelections {
  const normalizedTag = normalizeTagName(tag);
  if (!normalizedTag) {
    return selections;
  }

  if (!selections.included[normalizedTag] && !selections.excluded[normalizedTag]) {
    return selections;
  }

  const nextIncluded = { ...selections.included };
  const nextExcluded = { ...selections.excluded };
  delete nextIncluded[normalizedTag];
  delete nextExcluded[normalizedTag];

  return {
    included: nextIncluded,
    excluded: nextExcluded
  };
}

export function putTagInMode(
  selections: TagSelections,
  tag: string,
  mode: TagSelectionMode
): TagSelections {
  const normalizedTag = normalizeTagName(tag);
  const trimmedTag = tag.trim();
  if (!normalizedTag || !trimmedTag) {
    return selections;
  }

  const nextIncluded = { ...selections.included };
  const nextExcluded = { ...selections.excluded };
  delete nextIncluded[normalizedTag];
  delete nextExcluded[normalizedTag];

  if (mode === "include") {
    nextIncluded[normalizedTag] = trimmedTag;
  } else {
    nextExcluded[normalizedTag] = trimmedTag;
  }

  return {
    included: nextIncluded,
    excluded: nextExcluded
  };
}

export function buildFilterInput(selections: TagSelections): string {
  const includeTokens = sortTagsCaseInsensitive(Object.values(selections.included));
  const excludeTokens = sortTagsCaseInsensitive(Object.values(selections.excluded)).map(
    (tag) => `-${tag}`
  );

  return [...includeTokens, ...excludeTokens].join(" ");
}

export function applySingleSelectionAction(selections: TagSelections, tag: string): TagSelections {
  const currentMode = getTagMode(selections, tag);
  if (currentMode === null) {
    return putTagInMode(selections, tag, "include");
  }

  return removeTagSelection(selections, tag);
}

export function applyDoubleSelectionAction(selections: TagSelections, tag: string): TagSelections {
  const normalizedTag = normalizeTagName(tag);
  const currentMode = getTagMode(selections, tag);
  if (currentMode === "include") {
    return putTagInMode(selections, selections.included[normalizedTag] ?? tag, "exclude");
  }

  if (currentMode === "exclude") {
    return putTagInMode(selections, selections.excluded[normalizedTag] ?? tag, "include");
  }

  return putTagInMode(selections, tag, "exclude");
}
