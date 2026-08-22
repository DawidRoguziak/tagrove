import { deleteAsset } from "../../../api";
import type { AssetSummary, DeleteAssetSummary, SelectedAsset } from "../../../types";

type SetAssets = (updater: (previous: AssetSummary[]) => AssetSummary[]) => void;
type SetSelected = (updater: (previous: SelectedAsset | null) => SelectedAsset | null) => void;

interface DeleteLightboxAssetActionArgs {
  selected: SelectedAsset | null;
  setAssets: SetAssets;
  setSelected: SetSelected;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  onDeleted?: (assetId: number) => boolean | void;
  onResult?: (summary: DeleteAssetSummary) => void;
}

export async function deleteLightboxAssetAction({
  selected,
  setAssets,
  setSelected,
  refresh,
  refreshKnownTags,
  onDeleted,
  onResult
}: DeleteLightboxAssetActionArgs) {
  if (!selected) {
    return;
  }

  const summary = await deleteAsset(selected.id);
  onResult?.(summary);
  const accepted = onDeleted?.(selected.id);
  if (accepted === false) return;
  setAssets((previous) => previous.filter((asset) => asset.id !== selected.id));
  setSelected((previous) => (previous?.id === selected.id ? null : previous));
  await Promise.allSettled([refreshKnownTags(), refresh()]);
  return summary;
}
