import { readTagToken } from "./searchTokens";
import type { AssetSummary, SearchMetaFilter } from "../types";

export interface ParsedFilterTags {
  include: string[];
  exclude: string[];
}

export type SearchFilterValidationError =
  | "metaTagRequiresSolo"
  | "hasNoTagsInvalidCount"
  | "groupNameMissingValue"
  | "tagInvalidCharacters";

export interface ParsedSearchFilter {
  mode: "tags" | "meta";
  include: string[];
  exclude: string[];
  metaFilter: SearchMetaFilter | null;
  validationError: SearchFilterValidationError | null;
}

function createTagFilter(include: string[] = [], exclude: string[] = []): ParsedSearchFilter {
  return {
    mode: "tags",
    include,
    exclude,
    metaFilter: null,
    validationError: null
  };
}

function createValidationError(validationError: SearchFilterValidationError): ParsedSearchFilter {
  return {
    mode: "tags",
    include: [],
    exclude: [],
    metaFilter: null,
    validationError
  };
}

export function normalizeTags(input: string | string[]): string[] {
  const values = Array.isArray(input) ? input : input.split(/\s+/);
  return Array.from(
    new Set(
      values
        .map((item) => item.trim().toLowerCase())
        .filter(isValidTag)
    )
  );
}

export function isValidTag(tag: string): boolean {
  return Boolean(tag) && !/[,;\s\p{Cc}]/u.test(tag);
}

export function parseSearchFilter(input: string): ParsedSearchFilter {
  const trimmed = input.trim();
  if (!trimmed) {
    return createTagFilter();
  }

  const groupNameMatch = /^gn:(.*)$/i.exec(trimmed);
  if (groupNameMatch) {
    const groupName = groupNameMatch[1]?.trim() ?? "";
    if (!groupName) {
      return createValidationError("groupNameMissingValue");
    }

    return {
      mode: "meta",
      include: [],
      exclude: [],
      metaFilter: {
        type: "groupName",
        groupName
      },
      validationError: null
    };
  }

  const tokens = trimmed.split(/\s+/).filter(Boolean);
  const parsedTokens = tokens.map(readTagToken);
  if (tokens.some((token, index) => !parsedTokens[index].quoted && /^gn:/i.test(token))) {
    return createValidationError("metaTagRequiresSolo");
  }

  const hasNoTagsTokens = tokens.filter((token) => /^tags(?::.*)?$/i.test(token));

  if (hasNoTagsTokens.length > 0) {
    if (tokens.length !== 1 || hasNoTagsTokens.length !== 1) {
      return createValidationError("metaTagRequiresSolo");
    }

    const metaToken = hasNoTagsTokens[0] ?? "";
    const separatorIndex = metaToken.indexOf(":");
    if (separatorIndex < 0) {
      return {
        mode: "meta",
        include: [],
        exclude: [],
        metaFilter: {
          type: "hasNoTags",
          tagCount: 0
        },
        validationError: null
      };
    }

    const rawCount = metaToken.slice(separatorIndex + 1).trim();
    if (!/^\d+$/.test(rawCount)) {
      return createValidationError("hasNoTagsInvalidCount");
    }

    return {
      mode: "meta",
      include: [],
      exclude: [],
      metaFilter: {
        type: "hasNoTags",
        tagCount: Number.parseInt(rawCount, 10)
      },
      validationError: null
    };
  }

  if (parsedTokens.some((token) => !token.complete || !isValidTag(token.value))) {
    return createValidationError("tagInvalidCharacters");
  }

  return createTagFilter(
    [...new Set(parsedTokens.filter(token => !token.negative).map(token => token.value))],
    [...new Set(parsedTokens.filter(token => token.negative).map(token => token.value))]
  );
}

export function parseFilterTags(input: string): ParsedFilterTags {
  const parsed = parseSearchFilter(input);
  if (parsed.mode !== "tags" || parsed.validationError) {
    return {
      include: [],
      exclude: []
    };
  }

  return {
    include: parsed.include,
    exclude: parsed.exclude
  };
}

export function mapThumbs(items: AssetSummary[]): Record<number, string> {
  const output: Record<number, string> = {};
  for (const item of items) {
    if (item.thumb_path) {
      output[item.id] = item.thumb_path;
    }
  }
  return output;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = -1;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`;
}

export function formatDuration(durationMs: number | null): string {
  if (!durationMs || durationMs <= 0) return "-";
  const totalSeconds = Math.floor(durationMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}
