import { setAssetTags } from "../../../api";

import { updateSelectedTagsIfMatchingAsset } from "../../app/services/assetMutationService";

import type { SelectedAsset } from "../../../types";

import { normalizeTags } from "../../../utils/media";

type SetSelected = (updater: (previous: SelectedAsset | null) => SelectedAsset | null) => void;

interface SaveLightboxTagsActionArgs {
  selected: SelectedAsset | null;
  tagEditor: string[];
  setSelected: SetSelected;
  refreshKnownTags: () => Promise<string[]>;
  onSaved?: (assetId: number, tags: string[]) => boolean | void;
}

export async function saveLightboxTagsAction(
  { selected, tagEditor, setSelected, refreshKnownTags, onSaved }: SaveLightboxTagsActionArgs,
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
  setSelected((previous) => updateSelectedTagsIfMatchingAsset(previous, assetId, result.tags));
  void refreshKnownTags().catch(() => []);
  return result;
}
