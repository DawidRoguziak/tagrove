import { setAssetFavorite } from "../../../api";
import {
  updateAssetFavorite,
  updateSelectedFavoriteIfMatchingAsset
} from "../../app/services/assetMutationService";
import type { Asset } from "../../../types";

type SetAssets = (updater: (previous: Asset[]) => Asset[]) => void;
type SetSelected = (updater: (previous: Asset | null) => Asset | null) => void;

interface ToggleLightboxFavoriteActionArgs {
  selected: Asset | null;
  appliedFavoritesOnly: boolean;
  setAssets: SetAssets;
  setSelected: SetSelected;
  refresh: () => Promise<void>;
}

export async function toggleLightboxFavoriteAction({
  selected,
  appliedFavoritesOnly,
  setAssets,
  setSelected,
  refresh
}: ToggleLightboxFavoriteActionArgs) {
  if (!selected) {
    return;
  }

  const assetId = selected.id;
  const nextFavorite = !selected.is_favorite;
  await setAssetFavorite(assetId, nextFavorite);

  setAssets((previous) => updateAssetFavorite(previous, assetId, nextFavorite));
  setSelected((previous) => updateSelectedFavoriteIfMatchingAsset(previous, assetId, nextFavorite));

  if (appliedFavoritesOnly && !nextFavorite) {
    await refresh();
  }
}
