import { deleteAsset } from "../../../api";
import type { Asset } from "../../../types";

type SetAssets = (updater: (previous: Asset[]) => Asset[]) => void;
type SetSelected = (updater: (previous: Asset | null) => Asset | null) => void;

interface DeleteLightboxAssetActionArgs {
  selected: Asset | null;
  setAssets: SetAssets;
  setSelected: SetSelected;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  onDeleted?: (assetId: number) => boolean | void;
}

export async function deleteLightboxAssetAction({
  selected,
  setAssets,
  setSelected,
  refresh,
  refreshKnownTags,
  onDeleted
}: DeleteLightboxAssetActionArgs) {
  if (!selected) {
    return;
  }

  await deleteAsset(selected.id);
  const accepted = onDeleted?.(selected.id);
  if (accepted === false) return;
  setAssets((previous) => previous.filter((asset) => asset.id !== selected.id));
  setSelected((previous) => (previous?.id === selected.id ? null : previous));
  await refreshKnownTags();
  await refresh();
}
