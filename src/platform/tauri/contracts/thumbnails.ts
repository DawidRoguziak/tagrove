export type ThumbnailStreamEvent =
  | { event: "ready"; data: ThumbnailBatchItem }
  | { event: "failed"; data: { asset_id: number } }
  | { event: "stale"; data: { asset_id: number } }
  | { event: "done"; data: { ready: number; failed: number; stale: number } };

export interface ThumbnailRenderSummary {
  stale: number;
  generated: number;
  failed: number;
  skipped_failed: number;
  processed: number;
  total: number;
  cancelled: boolean;
}

export interface ThumbnailBatchItem {
  asset_id: number;
  thumb_path: string;
}
