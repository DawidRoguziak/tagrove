import type Fuse from "fuse.js";
import type { FuseResult, FuseResultMatch } from "fuse.js";
import type { ActiveToken, TagSuggestion } from "../types";

const DEFAULT_SEARCH_LIMIT = 20;
const DEFAULT_SUGGESTION_LIMIT = 8;

interface BuildTagSuggestionsParams {
  activeToken: ActiveToken | null;
  usedTags: Set<string>;
  excludedTags?: Set<string>;
  fuse: TagSearchIndex;
  searchLimit?: number;
  suggestionLimit?: number;
}

interface TagSearchIndex {
  search(query: string, options?: { limit?: number }): FuseResult<string>[];
}

function isIndexPair(entry: unknown): entry is readonly [number, number] {
  return (
    Array.isArray(entry) &&
    entry.length === 2 &&
    typeof entry[0] === "number" &&
    typeof entry[1] === "number"
  );
}

function collectIndices(result: FuseResult<string>): ReadonlyArray<readonly [number, number]> {
  return result.matches?.flatMap((match: FuseResultMatch) => match.indices).filter(isIndexPair) ?? [];
}

function isMetaFilterToken(query: string): boolean {
  return /^gn(?::.*)?$/i.test(query) || /^tags(?::.*)?$/i.test(query);
}

export function createTagFuse(knownTags: string[], FuseClass?: typeof Fuse): TagSearchIndex {
  if (!FuseClass) {
    return {
      search(query: string, options?: { limit?: number }) {
        const normalized = query.toLowerCase();
        return knownTags
          .map((item) => ({ item, index: item.toLowerCase().indexOf(normalized) }))
          .filter((entry) => entry.index >= 0)
          .slice(0, options?.limit)
          .map((entry) => ({
            item: entry.item,
            refIndex: 0,
            matches: [
              {
                indices: [[entry.index, entry.index + normalized.length - 1]],
                value: entry.item
              }
            ]
          }));
      }
    };
  }
  return new FuseClass(knownTags, {
    includeMatches: true,
    shouldSort: true,
    threshold: 0.35,
    minMatchCharLength: 1
  });
}

export function buildTagSuggestions({
  activeToken,
  usedTags,
  excludedTags = new Set(),
  fuse,
  searchLimit = DEFAULT_SEARCH_LIMIT,
  suggestionLimit = DEFAULT_SUGGESTION_LIMIT
}: BuildTagSuggestionsParams): TagSuggestion[] {
  if (!activeToken) {
    return [];
  }

  const query = activeToken.query.trim().toLowerCase();
  if (!activeToken.literal && !activeToken.negative && isMetaFilterToken(query)) {
    return [];
  }

  const seenValues = new Set<string>();
  const suggestions: TagSuggestion[] = [];

  let considered = 0;
  for (const result of fuse.search(query, { limit: searchLimit + excludedTags.size })) {
    const suggestionValue = result.item;
    const normalizedValue = suggestionValue.trim().toLowerCase();
    if (excludedTags.has(normalizedValue)) continue;
    if (considered++ >= searchLimit) break;

    if (usedTags.has(normalizedValue) && normalizedValue !== query) {
      continue;
    }

    if (seenValues.has(normalizedValue)) {
      continue;
    }

    seenValues.add(normalizedValue);
    suggestions.push({ value: suggestionValue, indices: collectIndices(result) });

    if (suggestions.length >= suggestionLimit) {
      break;
    }
  }

  return suggestions;
}

