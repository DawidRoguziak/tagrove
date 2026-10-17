import { serializeTagToken } from "../../../utils/searchTokens";
import type { TagListPage } from "../../../platform/tauri/contracts/tags";

export type TagSelectionMode = "include" | "exclude";

export interface TagSelections {
  included: ReadonlyMap<string, string>;
  excluded: ReadonlyMap<string, string>;
}

export function normalizeTagName(tag: string): string {
  return tag.trim().toLowerCase();
}

export function sortTagsCaseInsensitive(tags: string[]): string[] {
  const collator = new Intl.Collator(undefined, { sensitivity: "base" });
  return [...tags].sort(collator.compare);
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

  if (selections.included.has(normalizedTag)) {
    return "include";
  }

  if (selections.excluded.has(normalizedTag)) {
    return "exclude";
  }

  return null;
}

export function removeTagSelection(selections: TagSelections, tag: string): TagSelections {
  const normalizedTag = normalizeTagName(tag);
  if (!normalizedTag) {
    return selections;
  }

  if (!selections.included.has(normalizedTag) && !selections.excluded.has(normalizedTag)) {
    return selections;
  }

  const nextIncluded = new Map(selections.included);
  const nextExcluded = new Map(selections.excluded);
  nextIncluded.delete(normalizedTag);
  nextExcluded.delete(normalizedTag);

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

  const nextIncluded = new Map(selections.included);
  const nextExcluded = new Map(selections.excluded);
  nextIncluded.delete(normalizedTag);
  nextExcluded.delete(normalizedTag);

  if (mode === "include") {
    nextIncluded.set(normalizedTag, trimmedTag);
  } else {
    nextExcluded.set(normalizedTag, trimmedTag);
  }

  return {
    included: nextIncluded,
    excluded: nextExcluded
  };
}

export function buildFilterInput(selections: TagSelections): string {
  const includeTokens = sortTagsCaseInsensitive([...selections.included.values()]).map(tag => serializeTagToken(tag));
  const excludeTokens = sortTagsCaseInsensitive([...selections.excluded.values()]).map(
    (tag) => serializeTagToken(tag, true)
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
    return putTagInMode(selections, selections.included.get(normalizedTag) ?? tag, "exclude");
  }

  if (currentMode === "exclude") {
    return putTagInMode(selections, selections.excluded.get(normalizedTag) ?? tag, "include");
  }

  return putTagInMode(selections, tag, "exclude");
}
