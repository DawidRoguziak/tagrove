import { useEffect, useRef } from "react";
import { UiButton } from "../../UI/UiButton";
import { UiIcon } from "../../UI/UiIcon";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type { SectionOperationState, ScanSettingsController } from "../types";
import { useTranslation } from "react-i18next";

interface ScanSettingsSectionProps {
  highlighted?: boolean;
  scanRoots: string[];
  isOperationLocked: boolean;
  thumbnailBulkRunning: boolean;
  cancelThumbnailRunning: boolean;
  operationState: SectionOperationState;
  videoToolStatus?: ScanSettingsController["videoToolStatus"];
  onPickFolder: () => void;
  onRescanRoot: (path: string) => void;
  onRemoveRoot: (path: string) => void;
  onRescanAll: () => void;
  onRenderAllThumbnails: () => void;
  onRenderFailedThumbnails: () => void;
  onCancelThumbnailRender: () => void;
}

export function ScanSettingsSection({
  highlighted = false,
  scanRoots,
  isOperationLocked,
  thumbnailBulkRunning,
  cancelThumbnailRunning,
  operationState,
  videoToolStatus,
  onPickFolder,
  onRescanRoot,
  onRemoveRoot,
  onRescanAll,
  onRenderAllThumbnails,
  onRenderFailedThumbnails,
  onCancelThumbnailRender
}: ScanSettingsSectionProps) {
  const { t } = useTranslation();
  const sectionRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!highlighted) {
      return;
    }

    const sectionElement = sectionRef.current;
    if (!sectionElement || typeof sectionElement.scrollIntoView !== "function") {
      return;
    }

    sectionElement.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [highlighted]);

  return (
    <section
      ref={sectionRef}
      className={`grid gap-5 rounded-[var(--radius-surface)] border bg-[var(--surface-raised)] p-5 shadow-[var(--shadow-surface)] transition-all duration-300 sm:p-6 ${
        highlighted
          ? "border-primary/60 shadow-[0_0_0_3px_oklch(var(--p)/0.16),var(--shadow-surface)]"
          : "border-[var(--border-soft)]"
      }`}
      data-testid="scan-settings-section"
      data-highlighted={highlighted ? "true" : "false"}
    >
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="max-w-[680px]">
          <h2 className="m-0 text-lg">{t("settings.scan.heading")}</h2>
          <p className="m-0 mt-1 text-sm leading-relaxed text-base-content/60">
            {t("settings.scan.description")}
          </p>
        </div>
        <UiButton variant="primary" onClick={onPickFolder} disabled={isOperationLocked}>
          {t("settings.scan.chooseFolder")}
        </UiButton>
      </div>

      <div className="grid gap-3 border-t border-[var(--border-soft)] pt-4">
        <h3 className="m-0 text-xs font-bold uppercase tracking-[0.08em] text-base-content/55">{t("settings.scan.attachedPaths")}</h3>
        {scanRoots.length ? (
          <ul className="m-0 grid list-none gap-2 p-0">
            {scanRoots.map((path) => (
              <li key={path} className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-3 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-base-200/38 p-2.5 sm:grid-cols-[auto_minmax(0,1fr)_auto]">
                <span className="grid h-9 w-9 place-items-center rounded-lg bg-primary/10 text-primary">
                  <UiIcon name="folder" className="h-[18px] w-[18px]" />
                </span>
                <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-medium text-base-content/82" title={path}>
                  {path}
                </span>
                <div className="col-span-2 flex flex-wrap items-center justify-end gap-2 sm:col-span-1">
                  <UiButton variant="ghost" onClick={() => onRescanRoot(path)} disabled={isOperationLocked}>
                    {t("settings.scan.rescan")}
                  </UiButton>
                  <UiButton variant="danger" onClick={() => onRemoveRoot(path)} disabled={isOperationLocked}>
                    {t("settings.scan.remove")}
                  </UiButton>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <div className="p-5 text-base-content/65">{t("settings.scan.noPaths")}</div>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <UiButton variant="secondary" onClick={onRescanAll} disabled={!scanRoots.length || isOperationLocked}>
          {t("settings.scan.rescanAll")}
        </UiButton>
        <UiButton onClick={onRenderAllThumbnails} disabled={isOperationLocked}>
          {t("settings.scan.renderAllThumbnails")}
        </UiButton>
        <UiButton onClick={onRenderFailedThumbnails} disabled={isOperationLocked}>
          {t("settings.scan.retryFailedThumbnails")}
        </UiButton>
        {thumbnailBulkRunning && (
          <UiButton variant="danger" onClick={onCancelThumbnailRender} disabled={cancelThumbnailRunning}>
            {t("settings.scan.stopThumbnailRender")}
          </UiButton>
        )}
      </div>

      {videoToolStatus && (
        <div className="flex flex-wrap gap-3 text-xs text-base-content/65" data-testid="video-tool-status">
          <span>ffmpeg: {videoToolStatus.ffmpeg_available ? "OK" : "N/A"}</span>
          <span>ffprobe: {videoToolStatus.ffprobe_available ? "OK" : "N/A"}</span>
        </div>
      )}

      <SectionOperationStatus state={operationState} loaderTestId="scan-section-loader" />
    </section>
  );
}
