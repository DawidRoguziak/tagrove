import { createEffect, For } from "solid-js";
import { UiButton } from "../../../ui/controls/UiButton";
import { UiIcon } from "../../../ui/controls/UiIcon";
import { SectionOperationStatus } from "../SectionOperationStatus";
import type {
  SectionOperationState,
  ScanSettingsController
} from "../../../components/settings/types";
import { useLanguage } from "../../../i18n/solid";
interface ScanSettingsSectionProps {
  highlighted?: boolean;
  scanRoots: ScanSettingsController["scanRoots"];
  onSetAutoScan: (path: string, enabled: boolean) => void;
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
export function ScanSettingsSection(props: ScanSettingsSectionProps) {
  const { t } = useLanguage();
  let sectionElement: HTMLElement | undefined;
  createEffect(
    () => props.highlighted,
    (highlighted) => {
      if (!highlighted) {
        return;
      }
      if (!sectionElement || typeof sectionElement.scrollIntoView !== "function") {
        return;
      }
      const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
      sectionElement.scrollIntoView({
        behavior: reduceMotion ? "auto" : "smooth",
        block: "center"
      });
    }
  );
  return (
    <section
      ref={(el) => {
        sectionElement = el;
      }}
      class={`grid gap-5 rounded-[var(--radius-surface)] border bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)] motion-feedback ${
        (props.highlighted ?? false)
          ? "border-primary/60 shadow-[0_0_0_3px_oklch(var(--p)/0.16),var(--shadow-surface)]"
          : "border-[var(--border-soft)]"
      }`}
      data-testid="scan-settings-section"
      data-highlighted={(props.highlighted ?? false) ? "true" : "false"}
    >
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div class="max-w-[680px]">
          <h2 class="m-0 text-lg">{t("settings.scan.heading")}</h2>
          <p class="m-0 mt-1 text-sm leading-relaxed text-[var(--text-muted)]">
            {t("settings.scan.description")}
          </p>
        </div>
        <UiButton variant="primary" onClick={props.onPickFolder} disabled={props.isOperationLocked}>
          {t("settings.scan.chooseFolder")}
        </UiButton>
      </div>

      <div class="grid gap-3 border-t border-[var(--border-soft)] pt-4">
        <h3 class="m-0 text-xs font-semibold text-[var(--text-muted)]">
          {t("settings.scan.attachedPaths")}
        </h3>
        {props.scanRoots.length ? (
          <ul class="m-0 grid list-none gap-2 p-0">
            <For each={props.scanRoots}>
              {({ path, auto_scan_on_startup }) => (
                <li class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-3 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-base-200/38 p-2.5 sm:grid-cols-[auto_minmax(0,1fr)_auto]">
                  <span class="grid h-9 w-9 place-items-center rounded-[var(--radius-control)] bg-primary/10 text-primary-text">
                    <UiIcon name="folder" className="h-[18px] w-[18px]" />
                  </span>
                  <span
                    class="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-[13px] font-mono text-[var(--text-muted)]"
                    title={path}
                  >
                    {path}
                  </span>
                  <div class="col-span-2 flex flex-wrap items-center justify-end gap-2 sm:col-span-1">
                    <label class="flex cursor-pointer items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        class="toggle toggle-sm toggle-primary"
                        checked={auto_scan_on_startup}
                        disabled={props.isOperationLocked}
                        aria-label={t("settings.scan.autoScanLabel", { path })}
                        onChange={(event) => props.onSetAutoScan(path, event.currentTarget.checked)}
                      />
                      {t("settings.scan.autoScan")}
                    </label>
                    <UiButton
                      variant="ghost"
                      onClick={() => props.onRescanRoot(path)}
                      disabled={props.isOperationLocked}
                    >
                      {t("settings.scan.rescan")}
                    </UiButton>
                    <UiButton
                      variant="danger"
                      onClick={() => props.onRemoveRoot(path)}
                      disabled={props.isOperationLocked}
                    >
                      {t("settings.scan.remove")}
                    </UiButton>
                  </div>
                </li>
              )}
            </For>
          </ul>
        ) : (
          <div class="p-5 text-[var(--text-muted)]">{t("settings.scan.noPaths")}</div>
        )}
      </div>

      <div class="settings-operation">
        <p>{t("settings.scan.rescanDescription")}</p>
        <UiButton
          variant="secondary"
          onClick={props.onRescanAll}
          disabled={!props.scanRoots.length || props.isOperationLocked}
        >
          {t("settings.scan.rescanAll")}
        </UiButton>
      </div>
      <div class="settings-operation">
        <p>{t("settings.scan.thumbnailsDescription")}</p>
        <div class="settings-operation-actions">
          <UiButton onClick={props.onRenderAllThumbnails} disabled={props.isOperationLocked}>
            {t("settings.scan.renderAllThumbnails")}
          </UiButton>
          <UiButton onClick={props.onRenderFailedThumbnails} disabled={props.isOperationLocked}>
            {t("settings.scan.retryFailedThumbnails")}
          </UiButton>
          {props.thumbnailBulkRunning && (
            <UiButton
              variant="danger"
              onClick={props.onCancelThumbnailRender}
              disabled={props.cancelThumbnailRunning}
            >
              {t("settings.scan.stopThumbnailRender")}
            </UiButton>
          )}
        </div>
      </div>

      {props.videoToolStatus && (
        <div
          class="flex flex-wrap gap-3 text-xs text-[var(--text-muted)]"
          data-testid="video-tool-status"
        >
          <span>ffmpeg: {props.videoToolStatus.ffmpeg_available ? "OK" : "N/A"}</span>
          <span>ffprobe: {props.videoToolStatus.ffprobe_available ? "OK" : "N/A"}</span>
        </div>
      )}

      <SectionOperationStatus state={props.operationState} loaderTestId="scan-section-loader" />
    </section>
  );
}
