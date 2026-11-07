type ChangeEvent<T> = Event & {
  currentTarget: T;
};
type ButtonEvent<T> = MouseEvent & {
  currentTarget: T;
};
import { UiButton } from "../../ui/controls/UiButton";
import { ThumbnailImage } from "../../ui/controls/ThumbnailImage";
import { getFileNameFromPath } from "./duplicateValidationService";
import type { DuplicateResolutionChange } from "./types";
import { thumbnailUrl } from "../../platform/tauri/thumbnails";
import type { DuplicateAsset } from "../../platform/tauri/contracts/maintenance";
interface DuplicateAssetCardProps {
  asset: DuplicateAsset;
  renameValue: string;
  staged: DuplicateResolutionChange | null;
  isOperationLocked: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
  onRenameValueChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onGenerateUuidClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onQueueRenameClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onToggleDeleteClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onClearActionClick: (event: ButtonEvent<HTMLButtonElement>) => void;
}
export function DuplicateAssetCard(props: DuplicateAssetCardProps) {
  const currentName = () => getFileNameFromPath(props.asset.path);
  return (
    <article class="grid gap-3 rounded-[var(--radius-control)] border border-base-content/15 bg-base-200/35 p-2.5 md:grid-cols-[150px_minmax(0,1fr)]">
      <div class="relative mx-auto h-[150px] w-[150px] overflow-hidden rounded-[var(--radius-control)] border border-base-content/15 bg-base-100/80 shadow-[var(--shadow-tile)] md:mx-0">
        <ThumbnailImage
          className="block h-full w-full object-cover"
          src={thumbnailUrl(props.asset.id, props.asset.thumb_version)}
          retrySrc={(retry) => thumbnailUrl(props.asset.id, props.asset.thumb_version, { retry })}
          alt={props.asset.path}
          loading="lazy"
          draggable={false}
        />
        <div
          class="thumbnail-loader absolute inset-0 grid place-items-center bg-base-100/60"
          aria-hidden="true"
        >
          <span class="h-[24px] w-[24px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
        </div>
      </div>

      <div class="grid min-w-0 content-start gap-2.5">
        <div class="text-xs font-semibold text-[var(--text-muted)]">
          {props.t("settings.duplicates.resolve")}
        </div>
        <div class="break-all text-xs text-[var(--text-muted)]">{props.asset.path}</div>
        <div class="text-xs text-[var(--text-muted)]">
          {props.staged?.type === "rename"
            ? props.t("settings.duplicates.pendingRename", { name: props.staged.nextFileName })
            : props.staged?.type === "delete"
              ? props.t("settings.duplicates.pendingDelete")
              : props.t("settings.duplicates.pendingNone")}
        </div>
        <input
          data-asset-id={props.asset.id}
          value={props.renameValue}
          autocomplete="off"
          autocapitalize="none"
          autocorrect="off"
          spellcheck={false}
          onInput={props.onRenameValueChange}
          class="input input-sm h-10 w-full rounded-[var(--radius-control)] border border-base-content/20 bg-base-100/70"
          placeholder={props.t("settings.duplicates.renamePlaceholder")}
          disabled={props.isOperationLocked || props.staged?.type === "delete"}
        />
        <div class="flex flex-wrap gap-2">
          <UiButton
            data-asset-id={props.asset.id}
            data-current-name={currentName()}
            onClick={props.onGenerateUuidClick}
            disabled={props.isOperationLocked}
          >
            {props.t("settings.duplicates.generateUuid")}
          </UiButton>
          <UiButton
            data-asset-id={props.asset.id}
            onClick={props.onQueueRenameClick}
            disabled={props.isOperationLocked || !props.renameValue.trim()}
          >
            {props.t("settings.duplicates.queueRename")}
          </UiButton>
          <UiButton
            data-asset-id={props.asset.id}
            onClick={props.onToggleDeleteClick}
            disabled={props.isOperationLocked}
            variant={props.staged?.type === "delete" ? "default" : "danger"}
          >
            {props.staged?.type === "delete"
              ? props.t("settings.duplicates.unmarkDelete")
              : props.t("settings.duplicates.queueDelete")}
          </UiButton>
          {props.staged ? (
            <UiButton
              data-asset-id={props.asset.id}
              onClick={props.onClearActionClick}
              disabled={props.isOperationLocked}
            >
              {props.t("settings.duplicates.clearAction")}
            </UiButton>
          ) : null}
        </div>
      </div>
    </article>
  );
}
