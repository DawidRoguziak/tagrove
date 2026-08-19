import type { Asset } from "../../../types";

export interface BulkMediaGroupAssignment {
  assetId: number;
  mediaGroupOrder: number | null;
}

export function updateAssetTags(assets: Asset[], assetId: number, tags: string[]): Asset[] {
  return assets.map((asset) => (asset.id === assetId ? { ...asset, tags } : asset));
}

export function updateAssetFavorite(assets: Asset[], assetId: number, isFavorite: boolean): Asset[] {
  return assets.map((asset) => (asset.id === assetId ? { ...asset, is_favorite: isFavorite } : asset));
}

export function updateAssetMediaGroup(
  assets: Asset[],
  assetId: number,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): Asset[] {
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

export function updateSelectedTags(selected: Asset | null, tags: string[]): Asset | null {
  if (!selected) {
    return selected;
  }

  return {
    ...selected,
    tags
  };
}

export function updateSelectedTagsIfMatchingAsset(
  selected: Asset | null,
  assetId: number,
  tags: string[]
): Asset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedTags(selected, tags);
}

export function updateSelectedFavorite(selected: Asset | null, isFavorite: boolean): Asset | null {
  if (!selected) {
    return selected;
  }

  return {
    ...selected,
    is_favorite: isFavorite
  };
}

export function updateSelectedFavoriteIfMatchingAsset(
  selected: Asset | null,
  assetId: number,
  isFavorite: boolean
): Asset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedFavorite(selected, isFavorite);
}

export function updateSelectedMediaGroup(
  selected: Asset | null,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): Asset | null {
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
  selected: Asset | null,
  assetId: number,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): Asset | null {
  if (!selected || selected.id !== assetId) {
    return selected;
  }

  return updateSelectedMediaGroup(selected, mediaGroupKey, mediaGroupOrder);
}

export function applyBulkMediaGroupToAssets(
  assets: Asset[],
  mediaGroupKey: string | null,
  assignments: BulkMediaGroupAssignment[]
): Asset[] {
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
