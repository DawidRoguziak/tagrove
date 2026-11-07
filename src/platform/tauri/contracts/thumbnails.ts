export interface ThumbnailRenderSummary {
  stale: number;
  generated: number;
  failed: number;
  skipped_failed: number;
  processed: number;
  total: number;
  cancelled: boolean;
}
