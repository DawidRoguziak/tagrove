import { setAssetFavorite } from "../../../api";
import {
  updateAssetFavorite,
  updateSelectedFavoriteIfMatchingAsset
} from "../../app/services/assetMutationService";
import { favoriteMutationRequiresRefresh } from "../../app/services/libraryInvalidationService";
import type { AssetSummary, SelectedAsset } from "../../../types";

type SetAssets = (updater: (previous: AssetSummary[]) => AssetSummary[]) => void;
type SetSelected = (updater: (previous: SelectedAsset | null) => SelectedAsset | null) => void;

interface ToggleLightboxFavoriteActionArgs {
  selected: SelectedAsset | null;
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

  if (favoriteMutationRequiresRefresh(appliedFavoritesOnly, nextFavorite)) {
    await refresh();
  }
}
