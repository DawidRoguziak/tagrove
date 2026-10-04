import type { AssetSummary } from "../../platform/tauri/contracts/assets";

export interface SelectedAsset extends AssetSummary {
  path: string | null;
  size_bytes: number | null;
  tags: string[];
}
