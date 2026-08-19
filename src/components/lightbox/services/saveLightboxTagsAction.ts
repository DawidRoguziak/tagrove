import { setAssetTags } from "../../../api";
import {
  updateAssetTags,
  updateSelectedTagsIfMatchingAsset
} from "../../app/services/assetMutationService";
import type { Asset } from "../../../types";
import { normalizeTags } from "../../../utils/media";

type SetAssets = (updater: (previous: Asset[]) => Asset[]) => void;
type SetSelected = (updater: (previous: Asset | null) => Asset | null) => void;

interface SaveLightboxTagsActionArgs {
  selected: Asset | null;
  tagEditor: string[];
  setAssets: SetAssets;
  setSelected: SetSelected;
  refreshKnownTags: () => Promise<string[]>;
  onSaved?: (assetId: number, tags: string[]) => boolean | void;
}

export async function saveLightboxTagsAction(
  { selected, tagEditor, setAssets, setSelected, refreshKnownTags, onSaved }: SaveLightboxTagsActionArgs,
  nextTags?: string[]
) {
  if (!selected) {
    return;
  }

  const assetId = selected.id;
  const tags = normalizeTags(nextTags ?? tagEditor);
  const result = await setAssetTags(assetId, tags);

  const accepted = onSaved?.(assetId, result.tags);
  if (accepted === false) return result;
  setAssets((previous) => updateAssetTags(previous, assetId, result.tags));
  setSelected((previous) => updateSelectedTagsIfMatchingAsset(previous, assetId, result.tags));
  void refreshKnownTags().catch(() => []);
  return result;
}
