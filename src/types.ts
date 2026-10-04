export * from "./platform/tauri/contracts/media";
import type { AssetSummary } from "./platform/tauri/contracts/media";

export interface SelectedAsset extends AssetSummary {
  path: string | null;
  size_bytes: number | null;
  tags: string[];
}
