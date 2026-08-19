import { mergeAssetTagsBulk } from "../../../../api";
import type { BulkTagMergeSummary } from "../../../../types";
import { normalizeBulkTagList } from "./bulkTagMergeService";

interface ApplyBulkTagsActionArgs {
  assetIds: number[];
  tags: string[];
  refreshKnownTags: () => Promise<string[]>;
}

export async function applyBulkTagsAction({
  assetIds,
  tags,
  refreshKnownTags
}: ApplyBulkTagsActionArgs): Promise<BulkTagMergeSummary | null> {
  const normalizedAssetIds = Array.from(new Set(assetIds.filter((id) => Number.isInteger(id) && id > 0)));
  const normalizedTags = normalizeBulkTagList(tags);
  if (!normalizedAssetIds.length || !normalizedTags.length) return null;
  const result = await mergeAssetTagsBulk(normalizedAssetIds, normalizedTags);
  void refreshKnownTags().catch(() => []);
  return result;
}
