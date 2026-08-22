import { setAssetMediaGroup } from "../../../api";
import {
  updateAssetMediaGroup,
  updateSelectedMediaGroupIfMatchingAsset
} from "../../app/services/assetMutationService";
import type { AssetSummary, SelectedAsset } from "../../../types";

type SetAssets = (updater: (previous: AssetSummary[]) => AssetSummary[]) => void;
type SetSelected = (updater: (previous: SelectedAsset | null) => SelectedAsset | null) => void;
type SetText = (next: string) => void;

interface SaveLightboxMediaGroupActionArgs {
  selected: SelectedAsset | null;
  setAssets: SetAssets;
  setSelected: SetSelected;
  setMediaGroupKeyEditor?: SetText;
  setMediaGroupOrderEditor?: SetText;
}

export async function saveLightboxMediaGroupAction(
  { selected, setAssets, setSelected }: SaveLightboxMediaGroupActionArgs,
  next: { key: string | null; order: number | null }
) {
  if (!selected) {
    return;
  }

  const assetId = selected.id;

  await setAssetMediaGroup(assetId, next.key, next.order);
  setAssets((previous) => updateAssetMediaGroup(previous, assetId, next.key, next.order));
  setSelected((previous) =>
    updateSelectedMediaGroupIfMatchingAsset(previous, assetId, next.key, next.order)
  );
}
