
import { isValidTag } from "../../../../utils/media";

export function normalizeBulkTag(rawTag: string): string {
  const normalized = rawTag.trim().toLowerCase();
  return isValidTag(normalized) ? normalized : "";
}

export function normalizeBulkTagList(tags: string[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();

  for (const tag of tags) {
    const normalized = normalizeBulkTag(tag);
    if (!normalized || seen.has(normalized)) {
      continue;
    }

    seen.add(normalized);
    out.push(normalized);
  }

  return out;
}

export function mergeTagLists(existingTags: string[], incomingTags: string[]): string[] {
  const normalizedExisting = normalizeBulkTagList(existingTags);
  const normalizedIncoming = normalizeBulkTagList(incomingTags);

  if (!normalizedIncoming.length) {
    return normalizedExisting;
  }

  const seen = new Set(normalizedExisting);
  const merged = [...normalizedExisting];
  for (const tag of normalizedIncoming) {
    if (seen.has(tag)) {
      continue;
    }

    seen.add(tag);
    merged.push(tag);
  }

  return merged;
}
