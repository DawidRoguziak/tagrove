import type { AssetSummary } from "../../../../types";

export interface BulkMediaGroupAssignment {
  assetId: number;
  mediaGroupOrder: number | null;
}

export interface BulkGroupSelectionState {
  groupKey: string;
  hasConflictingGroups: boolean;
  orderedAssetIds: number[];
}

export function normalizeGroupKey(value: string): string {
  return value.trim();
}

export function uniqueOrderedAssetIds(assetIds: number[]): number[] {
  const seen = new Set<number>();
  const orderedIds: number[] = [];

  for (const assetId of assetIds) {
    if (!Number.isInteger(assetId) || assetId <= 0 || seen.has(assetId)) {
      continue;
    }

    seen.add(assetId);
    orderedIds.push(assetId);
  }

  return orderedIds;
}

export function buildBulkMediaGroupAssignments(
  assetIds: number[],
  hasGroupKey = true
): BulkMediaGroupAssignment[] {
  const orderedIds = uniqueOrderedAssetIds(assetIds);
  return orderedIds.map((assetId, index) => ({
    assetId,
    mediaGroupOrder: hasGroupKey ? index + 1 : null
  }));
}

function normalizedGroupIdentity(asset: AssetSummary): string | null {
  const key = normalizeGroupKey(asset.media_group_key ?? "");
  return key ? key.toLowerCase() : null;
}

export function deriveBulkGroupSelectionState(selectedAssets: AssetSummary[]): BulkGroupSelectionState {
  if (selectedAssets.length === 0) {
    return { groupKey: "", hasConflictingGroups: false, orderedAssetIds: [] };
  }

  if (selectedAssets.length === 1) {
    return {
      groupKey: normalizeGroupKey(selectedAssets[0]?.media_group_key ?? ""),
      hasConflictingGroups: false,
      orderedAssetIds: [selectedAssets[0]!.id]
    };
  }

  const identities = selectedAssets.map(normalizedGroupIdentity);
  const firstIdentity = identities[0] ?? null;
  const hasConflictingGroups = identities.some((identity) => identity !== firstIdentity);

  if (hasConflictingGroups) {
    return {
      groupKey: "",
      hasConflictingGroups: true,
      orderedAssetIds: selectedAssets.map((asset) => asset.id)
    };
  }

  if (firstIdentity === null) {
    return {
      groupKey: "",
      hasConflictingGroups: false,
      orderedAssetIds: selectedAssets.map((asset) => asset.id)
    };
  }

  const galleryIndexById = new Map(selectedAssets.map((asset, index) => [asset.id, index]));
  const orderedAssets = [...selectedAssets].sort((left, right) => {
    const leftOrder = left.media_group_order;
    const rightOrder = right.media_group_order;
    const leftHasOrder = typeof leftOrder === "number" && Number.isFinite(leftOrder);
    const rightHasOrder = typeof rightOrder === "number" && Number.isFinite(rightOrder);

    if (leftHasOrder && rightHasOrder && leftOrder !== rightOrder) {
      return leftOrder - rightOrder;
    }
    if (leftHasOrder !== rightHasOrder) {
      return leftHasOrder ? -1 : 1;
    }
    return (galleryIndexById.get(left.id) ?? 0) - (galleryIndexById.get(right.id) ?? 0);
  });

  return {
    groupKey: normalizeGroupKey(selectedAssets[0]?.media_group_key ?? ""),
    hasConflictingGroups: false,
    orderedAssetIds: orderedAssets.map((asset) => asset.id)
  };
}

export function reorderAssetIdsByDrop(
  orderedAssetIds: number[],
  draggedAssetId: number,
  targetAssetId: number
): number[] {
  if (draggedAssetId === targetAssetId) {
    return orderedAssetIds;
  }

  const fromIndex = orderedAssetIds.indexOf(draggedAssetId);
  const toIndex = orderedAssetIds.indexOf(targetAssetId);
  if (fromIndex < 0 || toIndex < 0) {
    return orderedAssetIds;
  }

  const next = [...orderedAssetIds];
  const [dragged] = next.splice(fromIndex, 1);
  if (typeof dragged !== "number") {
    return orderedAssetIds;
  }

  next.splice(toIndex, 0, dragged);
  return next;
}
