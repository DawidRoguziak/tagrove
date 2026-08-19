import type { ScanProgress, ThumbnailRenderSummary } from "../../../types";
import i18n from "../../../i18n";

const SCAN_PHASES = new Set(["counting", "scanning", "cleanup", "scan-empty", "scan-skip"]);

export function formatThumbnailSummary(label: string, summary: ThumbnailRenderSummary): string {
  const base = `${label}. ${i18n.t("settings.progress.thumbnailSummary.ready", {
    count: summary.generated
  })}, ${i18n.t("settings.progress.thumbnailSummary.failed", {
    count: summary.failed
  })}, ${i18n.t("settings.progress.thumbnailSummary.skippedFailed", {
    count: summary.skipped_failed
  })}`;
  if (summary.cancelled) {
    return `${base}. ${i18n.t("settings.progress.thumbnailSummary.cancelledAt", {
      processed: summary.processed,
      total: summary.total
    })}`;
  }
  return `${base}. ${i18n.t("settings.progress.thumbnailSummary.processed", {
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

function translateProgressByPhase(progress: ScanProgress): string | null {
  const params = {
    processed: progress.processed,
    total: progress.total
  };

  if (progress.phase === "counting") {
    return i18n.t("settings.progress.counting", params);
  }
  if (progress.phase === "scanning") {
    return i18n.t("settings.progress.scanning", params);
  }
  if (progress.phase === "cleanup") {
    return i18n.t("settings.progress.cleanup", params);
  }
  if (progress.phase === "scan-empty") {
    return i18n.t("settings.progress.scanEmpty");
  }
  if (progress.phase === "scan-skip") {
    return i18n.t("settings.progress.scanSkip");
  }
  if (progress.phase === "thumbs-start") {
    return i18n.t("settings.progress.thumbsStart");
  }
  if (progress.phase === "thumbs") {
    return i18n.t("settings.progress.thumbs", params);
  }
  if (progress.phase === "thumbs-done") {
    return i18n.t("settings.progress.thumbsDone");
  }
  if (progress.phase === "thumbs-failed-start") {
    return i18n.t("settings.progress.thumbsFailedStart");
  }
  if (progress.phase === "thumbs-failed") {
    return i18n.t("settings.progress.thumbsFailed", params);
  }
  if (progress.phase === "thumbs-failed-done") {
    return i18n.t("settings.progress.thumbsFailedDone");
  }
  if (progress.phase === "library-clear") {
    return i18n.t("settings.progress.libraryClear", params);
  }
  if (progress.phase === "library-clear-done") {
    return i18n.t("settings.progress.libraryClearDone");
  }
  if (progress.phase === "duplicates-scan") {
    return i18n.t("settings.progress.duplicatesScan", params);
  }
  if (progress.phase === "duplicates-scan-done") {
    return i18n.t("settings.progress.duplicatesScanDone");
  }

  return null;
}

function translateRawProgressMessage(rawMessage: string): string {
  const trimmed = rawMessage.trim();

  if (trimmed.startsWith("i18n:")) {
    const key = trimmed.slice(5).trim();
    if (key.length > 0) {
      return i18n.t(key);
    }
  }

  if (trimmed.startsWith("{")) {
    try {
      const parsed = JSON.parse(trimmed) as { key?: string; params?: Record<string, unknown> };
      if (typeof parsed.key === "string" && parsed.key.trim().length > 0) {
        return i18n.t(parsed.key, parsed.params ?? {});
      }
    } catch {
      // ignore invalid payload and keep raw message
    }
  }

  if (/^[a-z0-9_.-]+$/i.test(trimmed) && trimmed.includes(".")) {
    const translated = i18n.t(trimmed);
    if (translated !== trimmed) {
      return translated;
    }
  }

  return rawMessage;
}

export function translateProgressMessage(progress: ScanProgress): string {
  return translateProgressByPhase(progress) ?? translateRawProgressMessage(progress.message);
}
