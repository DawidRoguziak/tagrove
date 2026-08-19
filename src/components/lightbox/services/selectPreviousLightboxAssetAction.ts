import type { Asset } from "../../../types";

export function selectPreviousLightboxAssetAction(assets: Asset[], selected: Asset | null): Asset | null {
  if (!selected) {
    return selected;
  }

  const currentIndex = assets.findIndex((asset) => asset.id === selected.id);
  if (currentIndex <= 0) {
    return selected;
  }

  return assets[currentIndex - 1] ?? selected;
}
