import type { AssetSummary, SelectedAsset } from "../../../types";

export interface BulkMediaGroupAssignment {
  assetId: number;
  mediaGroupOrder: number | null;
}

export function updateAssetFavorite(assets: AssetSummary[], assetId: number, isFavorite: boolean): AssetSummary[] {
  return assets.map((asset) => (asset.id === assetId ? { ...asset, is_favorite: isFavorite } : asset));
}

export function updateAssetMediaGroup(
  assets: AssetSummary[],
  assetId: number,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): AssetSummary[] {
  return assets.map((asset) =>
    asset.id === assetId
      ? {
          ...asset,
          media_group_key: mediaGroupKey,
          media_group_order: mediaGroupOrder
        }
      : asset
  );
}

export function updateSelectedTags(selected: SelectedAsset | null, tags: string[]): SelectedAsset | null {
  if (!selected) {
    return selected;
  }

  return {
    ...selected,
    tags
  };
}

export function updateSelectedTagsIfMatchingAsset(
  selected: SelectedAsset | null,
  assetId: number,
  tags: string[]
): SelectedAsset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedTags(selected, tags);
}

export function updateSelectedFavorite(selected: SelectedAsset | null, isFavorite: boolean): SelectedAsset | null {
  if (!selected) {
    return selected;
  }

  return {
    ...selected,
    is_favorite: isFavorite
  };
}

export function updateSelectedFavoriteIfMatchingAsset(
  selected: SelectedAsset | null,
  assetId: number,
  isFavorite: boolean
): SelectedAsset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedFavorite(selected, isFavorite);
}

export function updateSelectedMediaGroup(
  selected: SelectedAsset | null,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): SelectedAsset | null {
  if (!selected) {
    return selected;
  }

  return {
    ...selected,
    media_group_key: mediaGroupKey,
    media_group_order: mediaGroupOrder
  };
}

export function updateSelectedMediaGroupIfMatchingAsset(
  selected: SelectedAsset | null,
  assetId: number,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): SelectedAsset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedMediaGroup(selected, mediaGroupKey, mediaGroupOrder);
}

export function applyBulkMediaGroupToAssets(
  assets: AssetSummary[],
  mediaGroupKey: string | null,
  assignments: BulkMediaGroupAssignment[]
): AssetSummary[] {
  if (!assets.length || !assignments.length) {
    return assets;
  }

  const orderByAssetId = new Map(assignments.map((assignment) => [assignment.assetId, assignment.mediaGroupOrder]));

  return assets.map((asset) => {
    if (!orderByAssetId.has(asset.id)) {
      return asset;
    }
    const nextOrder = orderByAssetId.get(asset.id);

    if (asset.media_group_key === mediaGroupKey && asset.media_group_order === (nextOrder ?? null)) {
      return asset;
    }

    return {
      ...asset,
      media_group_key: mediaGroupKey,
      media_group_order: nextOrder ?? null
    };
  });
}
