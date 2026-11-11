import type { ScanProgress } from "../../platform/tauri/contracts/scanning";
import type { ThumbnailRenderSummary } from "../../platform/tauri/contracts/thumbnails";
import i18n from "../../i18n/core";
import { translateBackendText } from "../../i18n/backendText";

type Translate = (key: string, options?: Record<string, unknown>) => string;
const i18nTranslate: Translate = (key, options) => i18n.t(key, options);

const SCAN_PHASES = new Set(["counting", "scanning", "cleanup", "scan-empty", "scan-skip"]);

export function formatThumbnailSummary(
  label: string,
  summary: ThumbnailRenderSummary,
  t: Translate = i18nTranslate
): string {
  const base = `${label}. ${t("settings.progress.thumbnailSummary.ready", {
    count: summary.generated
  })}, ${t("settings.progress.thumbnailSummary.failed", {
    count: summary.failed
  })}, ${t("settings.progress.thumbnailSummary.skippedFailed", {
    count: summary.skipped_failed
  })}`;
  if (summary.cancelled) {
    return `${base}. ${t("settings.progress.thumbnailSummary.cancelledAt", {
      processed: summary.processed,
      total: summary.total
    })}`;
  }
  return `${base}. ${t("settings.progress.thumbnailSummary.processed", {
    processed: summary.processed,
    total: summary.total
  })}`;
}

export function formatProgressLabel(progress: ScanProgress): string {
  return `${progress.processed}/${progress.total}`;
}

export function isScanProgressPhase(phase: string): boolean {
  return SCAN_PHASES.has(phase);
}

export function isThumbnailProgressPhase(phase: string): boolean {
  return phase.startsWith("thumbs");
}

export function isLibraryClearProgressPhase(phase: string): boolean {
  return phase.startsWith("library-clear");
}

export function isDuplicateScanProgressPhase(phase: string): boolean {
  return phase.startsWith("duplicates-scan");
}

function translateProgressByPhase(progress: ScanProgress, t: Translate): string | null {
  const params = {
    processed: progress.processed,
    total: progress.total
  };

  if (progress.phase === "counting") {
    return t("settings.progress.counting", params);
  }
  if (progress.phase === "scanning") {
    return t("settings.progress.scanning", params);
  }
  if (progress.phase === "cleanup") {
    return t("settings.progress.cleanup", params);
  }
  if (progress.phase === "scan-empty") {
    return t("settings.progress.scanEmpty");
  }
  if (progress.phase === "scan-skip") {
    return t("settings.progress.scanSkip");
  }
  if (progress.phase === "thumbs-start") {
    return t("settings.progress.thumbsStart");
  }
  if (progress.phase === "thumbs") {
    return t("settings.progress.thumbs", params);
  }
  if (progress.phase === "thumbs-done") {
    return t("settings.progress.thumbsDone");
  }
  if (progress.phase === "thumbs-failed-start") {
    return t("settings.progress.thumbsFailedStart");
  }
  if (progress.phase === "thumbs-failed") {
    return t("settings.progress.thumbsFailed", params);
  }
  if (progress.phase === "thumbs-failed-done") {
    return t("settings.progress.thumbsFailedDone");
  }
  if (progress.phase === "library-clear") {
    return t("settings.progress.libraryClear", params);
  }
  if (progress.phase === "library-clear-done") {
    return t("settings.progress.libraryClearDone");
  }
  if (progress.phase === "duplicates-scan") {
    return t("settings.progress.duplicatesScan", params);
  }
  if (progress.phase === "duplicates-scan-done") {
    return t("settings.progress.duplicatesScanDone");
  }

  return null;
}

export function translateProgressMessage(
  progress: ScanProgress,
  t: Translate = i18nTranslate
): string {
  return translateProgressByPhase(progress, t) ?? translateBackendText(progress.message, t);
}
