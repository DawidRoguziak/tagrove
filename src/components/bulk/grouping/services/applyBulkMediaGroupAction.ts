import { setAssetsMediaGroupBulk } from "../../../../api";
import type { AssetSummary, BulkMediaGroupSummary } from "../../../../types";
import {
  applyBulkMediaGroupToAssets,
  type BulkMediaGroupAssignment
} from "../../../app/services/assetMutationService";
import {
  buildBulkMediaGroupAssignments,
  normalizeGroupKey,
  uniqueOrderedAssetIds
} from "./bulkGroupOrderService";

type SetAssets = (updater: (previous: AssetSummary[]) => AssetSummary[]) => void;

interface ApplyBulkMediaGroupActionArgs {
  assetIdsInOrder: number[];
  groupKey: string;
  preservedSingleOrder?: number | null;
  setAssets: SetAssets;
}

export interface ApplyBulkMediaGroupPayload {
  normalizedGroupKey: string | null;
  assignments: BulkMediaGroupAssignment[];
}

export function buildApplyBulkMediaGroupPayload(
  assetIdsInOrder: number[],
  groupKey: string,
  preservedSingleOrder: number | null = null
): ApplyBulkMediaGroupPayload | null {
  const orderedAssetIds = uniqueOrderedAssetIds(assetIdsInOrder);
  if (!orderedAssetIds.length) {
    return null;
  }

  const normalizedGroupKey = normalizeGroupKey(groupKey) || null;
  const assignments = buildBulkMediaGroupAssignments(orderedAssetIds, normalizedGroupKey !== null);
  if (
    normalizedGroupKey !== null &&
    assignments.length === 1 &&
    typeof preservedSingleOrder === "number" &&
    Number.isFinite(preservedSingleOrder)
  ) {
    assignments[0] = { ...assignments[0]!, mediaGroupOrder: preservedSingleOrder };
  }

  return {
    normalizedGroupKey,
    assignments
  };
}

export async function applyBulkMediaGroupAction({
  assetIdsInOrder,
  groupKey,
  preservedSingleOrder = null,
  setAssets
}: ApplyBulkMediaGroupActionArgs): Promise<BulkMediaGroupSummary | undefined> {
  const payload = buildApplyBulkMediaGroupPayload(assetIdsInOrder, groupKey, preservedSingleOrder);
  if (!payload) {
    return;
  }

  const result = await setAssetsMediaGroupBulk(payload.assignments, payload.normalizedGroupKey);
  const processed = new Set(result.processed_asset_ids);

  setAssets((previous) =>
    applyBulkMediaGroupToAssets(previous, payload.normalizedGroupKey, payload.assignments.filter(item => processed.has(item.assetId)))
  );
  return result;
}
