import { setAssetMediaGroup } from "../../../api";
import {
  updateAssetMediaGroup,
  updateSelectedMediaGroupIfMatchingAsset
} from "../../app/services/assetMutationService";
import type { Asset } from "../../../types";

type SetAssets = (updater: (previous: Asset[]) => Asset[]) => void;
type SetSelected = (updater: (previous: Asset | null) => Asset | null) => void;
type SetText = (next: string) => void;

interface SaveLightboxMediaGroupActionArgs {
  selected: Asset | null;
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
