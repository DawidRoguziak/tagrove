import { useThumbnailSubscription } from "../UI/ThumbnailSubscription";
import type { ChangeEvent, MouseEvent } from "react";
import { UiButton } from "../UI/UiButton";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../UI/ThumbnailImage";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { getFileNameFromPath } from "./services/duplicateValidationService";
import type { DuplicateResolutionChange } from "./types";
import { toMediaSrc } from "../../api";
import type { DuplicateAsset } from "../../types";

interface DuplicateAssetCardProps {
  asset: DuplicateAsset;
  renameValue: string;
  staged: DuplicateResolutionChange | null;
  thumbPath?: string;
  isRendering: boolean;
  isOperationLocked: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
  onRenameValueChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onGenerateUuidClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onQueueRenameClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onToggleDeleteClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onClearActionClick: (event: MouseEvent<HTMLButtonElement>) => void;
}

export function DuplicateAssetCard({
  asset,
  renameValue,
  staged,
  thumbPath: fallbackPath,
  isRendering: fallbackRendering,
  isOperationLocked,
  t,
  onRenameValueChange,
  onGenerateUuidClick,
  onQueueRenameClick,
  onToggleDeleteClick,
  onClearActionClick
}: DuplicateAssetCardProps) {
  const { path: thumbPath, rendering } = useThumbnailSubscription(asset.id, fallbackPath, fallbackRendering);
  const isRendering = rendering && !thumbPath;
  const currentName = getFileNameFromPath(asset.path);
  const previewSrc = thumbPath ? toMediaSrc(thumbPath) : TRANSPARENT_THUMBNAIL_SRC;

  return (
    <article className="grid gap-3 rounded-[var(--radius-control)] border border-base-content/15 bg-base-200/35 p-2.5 md:grid-cols-[150px_minmax(0,1fr)]">
      <div className="relative mx-auto h-[150px] w-[150px] overflow-hidden rounded-[var(--radius-control)] border border-base-content/15 bg-base-100/80 shadow-[var(--shadow-tile)] md:mx-0">
        <ThumbnailImage
          className="block h-full w-full object-cover"
          src={previewSrc}
          alt={asset.path}
          loading="lazy"
          draggable={false}
        />
        {isRendering ? (
          <div
            className="absolute inset-0 grid place-items-center bg-base-100/60"
            aria-hidden="true"
          >
            <span className="h-[24px] w-[24px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
          </div>
        ) : null}
      </div>

      <div className="grid min-w-0 content-start gap-2.5">
        <div className="text-[11px] font-semibold uppercase tracking-[0.12em] text-base-content/60">
          {t("settings.duplicates.resolve")}
        </div>
        <div className="break-all text-xs text-base-content/70">{asset.path}</div>
        <div className="text-xs text-base-content/75">
          {staged?.type === "rename"
            ? t("settings.duplicates.pendingRename", { name: staged.nextFileName })
            : staged?.type === "delete"
              ? t("settings.duplicates.pendingDelete")
              : t("settings.duplicates.pendingNone")}
        </div>
        <input
          data-asset-id={asset.id}
          value={renameValue}
          {...browserAssistDisabledProps}
          onChange={onRenameValueChange}
          className="input input-sm h-10 w-full rounded-[var(--radius-control)] border border-base-content/20 bg-base-100/70"
          placeholder={t("settings.duplicates.renamePlaceholder")}
          disabled={isOperationLocked || staged?.type === "delete"}
        />
        <div className="flex flex-wrap gap-2">
          <UiButton
            data-asset-id={asset.id}
            data-current-name={currentName}
            onClick={onGenerateUuidClick}
            disabled={isOperationLocked}
          >
            {t("settings.duplicates.generateUuid")}
          </UiButton>
          <UiButton
            data-asset-id={asset.id}
            onClick={onQueueRenameClick}
            disabled={isOperationLocked || !renameValue.trim()}
          >
            {t("settings.duplicates.queueRename")}
          </UiButton>
          <UiButton
            data-asset-id={asset.id}
            onClick={onToggleDeleteClick}
            disabled={isOperationLocked}
            variant={staged?.type === "delete" ? "default" : "danger"}
          >
            {staged?.type === "delete"
              ? t("settings.duplicates.unmarkDelete")
              : t("settings.duplicates.queueDelete")}
          </UiButton>
          {staged ? (
            <UiButton
              data-asset-id={asset.id}
              onClick={onClearActionClick}
              disabled={isOperationLocked}
            >
              {t("settings.duplicates.clearAction")}
            </UiButton>
          ) : null}
        </div>
      </div>
    </article>
  );
}
