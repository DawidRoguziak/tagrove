import type { Asset } from "../../../types";

export function selectNextLightboxAssetAction(assets: Asset[], selected: Asset | null): Asset | null {
  if (!selected) {
    return selected;
  }

  const currentIndex = assets.findIndex((asset) => asset.id === selected.id);
  if (currentIndex < 0 || currentIndex >= assets.length - 1) {
    return selected;
  }

  return assets[currentIndex + 1] ?? selected;
}
