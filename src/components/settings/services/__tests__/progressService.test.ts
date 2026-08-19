import { describe, expect, it } from "vitest";
import {
  formatProgressLabel,
  formatThumbnailSummary,
  isDuplicateScanProgressPhase,
  isLibraryClearProgressPhase,
  isScanProgressPhase,
  isThumbnailProgressPhase
} from "../progressService";

describe("progressService", () => {
  it("formats thumbnail summary for completed render", () => {
    const message = formatThumbnailSummary("All thumbnails", {
      generated: 8,
      failed: 1,
      skipped_failed: 2,
      processed: 11,
      total: 11,
      cancelled: false
    });

    expect(message).toBe(
      "All thumbnails. Ready: 8, failed: 1, skipped failed: 2. Processed: 11/11."
    );
  });

  it("formats thumbnail summary for cancelled render", () => {
    const message = formatThumbnailSummary("Retry failed", {
      generated: 3,
      failed: 0,
      skipped_failed: 5,
      processed: 8,
      total: 20,
      cancelled: true
    });

    expect(message).toBe(
      "Retry failed. Ready: 3, failed: 0, skipped failed: 5. Cancelled at 8/20."
    );
  });

  it("formats progress label from processed and total", () => {
    expect(
      formatProgressLabel({
        phase: "scanning",
        processed: 12,
        total: 40,
        message: "Scanning"
      })
    ).toBe("12/40");
  });

  it("matches scan, thumbnail and library clear phases", () => {
    expect(isScanProgressPhase("counting")).toBe(true);
    expect(isScanProgressPhase("cleanup")).toBe(true);
    expect(isScanProgressPhase("library-clear")).toBe(false);

    expect(isThumbnailProgressPhase("thumbs-rendering")).toBe(true);
    expect(isThumbnailProgressPhase("scanning")).toBe(false);

    expect(isLibraryClearProgressPhase("library-clear")).toBe(true);
    expect(isLibraryClearProgressPhase("library-clear-done")).toBe(true);
    expect(isLibraryClearProgressPhase("thumbs-batch")).toBe(false);

    expect(isDuplicateScanProgressPhase("duplicates-scan")).toBe(true);
    expect(isDuplicateScanProgressPhase("duplicates-scan-done")).toBe(true);
    expect(isDuplicateScanProgressPhase("scanning")).toBe(false);
  });
});
