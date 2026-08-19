import type { ChangeEvent, MouseEvent } from "react";
import { isDuplicateGroupResolved } from "./services/duplicateValidationService";
import type { DuplicateAsset, DuplicateGroup } from "../../types";
import type { DuplicateResolutionChange } from "./types";
import { DuplicateAssetCard } from "./DuplicateAssetCard";

interface DuplicateGroupCardProps {
  group: DuplicateGroup;
  renameValues: Record<number, string>;
  stagedChangesByAssetId: Record<number, DuplicateResolutionChange>;
  thumbs?: Record<number, string>;
  renderingThumbnailIds?: Record<number, true>;
  isOperationLocked: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
  onRenameValueChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onGenerateUuidClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onQueueRenameClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onToggleDeleteClick: (event: MouseEvent<HTMLButtonElement>) => void;
  onClearActionClick: (event: MouseEvent<HTMLButtonElement>) => void;
}

export function DuplicateGroupCard({
  group,
  renameValues,
  stagedChangesByAssetId,
  thumbs = {},
  renderingThumbnailIds = {},
  isOperationLocked,
  t,
  onRenameValueChange,
  onGenerateUuidClick,
  onQueueRenameClick,
  onToggleDeleteClick,
  onClearActionClick
}: DuplicateGroupCardProps) {
  const isResolved = isDuplicateGroupResolved(group, stagedChangesByAssetId);

  return (
    <section
      className={`grid gap-2 rounded-xl border p-3 ${
        isResolved ? "border-primary/40 bg-primary/10" : "border-base-content/20 bg-base-100/75"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <h4 className="m-0 text-sm">{group.file_name}</h4>
        {isResolved ? (
          <span className="rounded-full border border-primary/40 bg-primary/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-primary">
            {t("settings.duplicates.readyBadge")}
          </span>
        ) : null}
      </div>

      {group.assets.map((asset) => {
        const currentName = getCurrentName(asset);
        const renameValue = renameValues[asset.id] ?? currentName;
        const staged = stagedChangesByAssetId[asset.id] ?? null;
        const thumbPath = thumbs[asset.id];
        const isRendering = Boolean(renderingThumbnailIds[asset.id]) && !thumbPath;

        return (
          <DuplicateAssetCard
            key={asset.id}
            asset={asset}
            renameValue={renameValue}
            staged={staged}
            thumbPath={thumbPath}
            isRendering={isRendering}
            isOperationLocked={isOperationLocked}
            t={t}
            onRenameValueChange={onRenameValueChange}
            onGenerateUuidClick={onGenerateUuidClick}
            onQueueRenameClick={onQueueRenameClick}
            onToggleDeleteClick={onToggleDeleteClick}
            onClearActionClick={onClearActionClick}
          />
        );
      })}
    </section>
  );
}

function getCurrentName(asset: DuplicateAsset) {
  return asset.path.split(/[\\/]/).pop() ?? asset.path;
}
