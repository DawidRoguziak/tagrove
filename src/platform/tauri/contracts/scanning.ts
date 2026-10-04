export interface ScanRoot {
  path: string;
  auto_scan_on_startup: boolean;
}

export interface ScanSummary {
  completion: "complete" | "partial";
  indexed: number;
  removed: number;
  failed: number;
}

export interface RemoveRootSummary {
  removed_assets: number;
  removed_thumbnails: number;
}

export interface ScanProgress {
  phase: "counting" | "scanning" | "cleanup" | string;
  processed: number;
  total: number;
  message: string;
}
