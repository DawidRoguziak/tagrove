import type { Asset } from "../../../../types";

export function normalizeBulkTag(rawTag: string): string {
  return rawTag.trim().toLowerCase();
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

export function mergeBulkTagsInAssets(
  assets: Asset[],
  selectedAssetIds: Set<number>,
  incomingTags: string[]
): Asset[] {
  if (!assets.length || !selectedAssetIds.size) {
    return assets;
  }

  const normalizedIncoming = normalizeBulkTagList(incomingTags);
  if (!normalizedIncoming.length) {
    return assets;
  }

  return assets.map((asset) => {
    if (!selectedAssetIds.has(asset.id)) {
      return asset;
    }

    const mergedTags = mergeTagLists(asset.tags, normalizedIncoming);
    const isUnchanged =
      mergedTags.length === asset.tags.length &&
      mergedTags.every((tag, index) => tag === asset.tags[index]);

    if (isUnchanged) {
      return asset;
    }

    return {
      ...asset,
      tags: mergedTags
    };
  });
}
