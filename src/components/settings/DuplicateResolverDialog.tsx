import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useTranslation } from "react-i18next";
import { UiButton } from "../UI/UiButton";
import { UiModal } from "../UI/UiModal";
import { DuplicateGroupCard } from "./DuplicateGroupCard";
import { useDuplicateResolverHandlers } from "./hooks/useDuplicateResolverHandlers";
import { useDuplicateResolverState } from "./hooks/useDuplicateResolverState";
import { SectionOperationStatus } from "./SectionOperationStatus";
import type { DuplicateGroup } from "../../types";
import type { DuplicateResolutionChange, SectionOperationState } from "./types";

interface DuplicateResolverDialogProps {
  open: boolean;
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  groups: DuplicateGroup[];
  thumbs?: Record<number, string>;
  renderingThumbnailIds?: Record<number, true>;
  onQueueThumbnailsByIds?: (assetIds: number[]) => void;
  onClose: () => void;
  onRescan: () => void;
  onSaveAll: (changes: DuplicateResolutionChange[]) => Promise<void>;
}

function getExtension(fileName: string): string {
  const dotIndex = fileName.lastIndexOf(".");
  if (dotIndex <= 0) {
    return "";
  }
  return fileName.slice(dotIndex);
}

function createUuidName(baseFileName: string): string {
  const extension = getExtension(baseFileName);
  const uuid = window.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `${uuid}${extension}`;
}

export function DuplicateResolverDialog({
  open,
  isOperationLocked,
  operationState,
  groups,
  thumbs = {},
  renderingThumbnailIds = {},
  onQueueThumbnailsByIds,
  onClose,
  onRescan,
  onSaveAll
}: DuplicateResolverDialogProps) {
  const { t } = useTranslation();
  const state = useDuplicateResolverState(open, groups);
  const groupsScrollRef = useRef<HTMLDivElement | null>(null);
  const groupVirtualizer = useVirtualizer({
    count: groups.length,
    getScrollElement: () => groupsScrollRef.current,
    estimateSize: () => 360,
    overscan: 2,
    initialRect: { width: 0, height: 720 }
  });
  const virtualGroups = groupVirtualizer.getVirtualItems();
  const renderedGroups = virtualGroups.length > 0
    ? virtualGroups
    : groups.slice(0, 3).map((_, index) => ({
        index,
        key: `initial-${index}`,
        start: index * 360
      }));

  useEffect(() => {
    if (!open || !onQueueThumbnailsByIds) {
      return;
    }

    const assetIds = Array.from(new Set(virtualGroups.flatMap((item) =>
      (groups[item.index]?.assets ?? []).map((asset) => asset.id)
    )));
    if (!assetIds.length) {
      return;
    }

    onQueueThumbnailsByIds(assetIds);
  }, [groups, onQueueThumbnailsByIds, open, virtualGroups]);
  const hasPendingChanges = state.stagedChanges.length > 0;
  const canSaveAll = !isOperationLocked && hasPendingChanges && state.validationResult.valid;

  const handlers = useDuplicateResolverHandlers({
    stagedChanges: state.stagedChanges,
    stagedChangesByAssetId: state.stagedChangesByAssetId,
    renameValues: state.renameValues,
    setRenameValues: state.setRenameValues,
    setStagedChangesByAssetId: state.setStagedChangesByAssetId,
    onSaveAll,
    createUuidName
  });

  if (!open) {
    return null;
  }

  return (
    <UiModal
      open
      onClose={onClose}
      closeOnOverlayClick={!isOperationLocked}
      closeOnEscape={!isOperationLocked}
      size="large"
      labelledBy="duplicate-resolver-heading"
      contentClassName="grid h-[min(88vh,900px)] grid-rows-[auto_auto_auto_1fr] gap-3"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="grid gap-1">
          <h3 id="duplicate-resolver-heading" className="m-0">
            {t("settings.duplicates.resolverHeading")}
          </h3>
          <p className="m-0 text-xs text-base-content/65">
            {t("settings.duplicates.resolverSummary", {
              groups: groups.length,
              assets: state.duplicateAssetCount
            })}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <UiButton onClick={handlers.handleSaveAllClick} disabled={!canSaveAll}>
            {t("settings.duplicates.saveAll")}
          </UiButton>
          <UiButton onClick={state.resetForm} disabled={isOperationLocked}>
            {t("settings.duplicates.discardChanges")}
          </UiButton>
          <UiButton onClick={onRescan} disabled={isOperationLocked}>
            {t("settings.scan.rescan")}
          </UiButton>
          <UiButton onClick={onClose} disabled={isOperationLocked}>
            {t("common.close")}
          </UiButton>
        </div>
      </div>

      <SectionOperationStatus state={operationState} loaderTestId="duplicates-modal-loader" />

      <div className="text-xs text-base-content/75">
        {!hasPendingChanges
          ? t("settings.duplicates.queueHint")
          : state.validationResult.valid
            ? t("settings.duplicates.queuedReady", { count: state.stagedChanges.length })
            : state.validationResult.firstError}
      </div>

      <div ref={groupsScrollRef} className="overflow-auto pr-1">
        {!groups.length && !operationState.loading ? (
          <div className="rounded-xl border border-base-content/20 bg-base-200/40 p-3 text-sm text-base-content/70">
            {t("settings.duplicates.noGroups")}
          </div>
        ) : null}

        <div className="relative" style={{ height: groupVirtualizer.getTotalSize() }}>
          {renderedGroups.map((item) => {
            const group = groups[item.index];
            if (!group) return null;
            return (
              <div
                key={item.key}
                ref={groupVirtualizer.measureElement}
                data-index={item.index}
                className="absolute left-0 top-0 w-full pb-2"
                style={{ transform: `translateY(${item.start}px)` }}
              >
                <DuplicateGroupCard
                  group={group}
                  renameValues={state.renameValues}
                  stagedChangesByAssetId={state.stagedChangesByAssetId}
                  thumbs={thumbs}
                  renderingThumbnailIds={renderingThumbnailIds}
                  isOperationLocked={isOperationLocked}
                  t={t}
                  onRenameValueChange={handlers.handleRenameValueChange}
                  onGenerateUuidClick={handlers.handleGenerateUuidClick}
                  onQueueRenameClick={handlers.handleQueueRenameClick}
                  onToggleDeleteClick={handlers.handleToggleDeleteClick}
                  onClearActionClick={handlers.handleClearActionClick}
                />
              </div>
            );
          })}
        </div>
      </div>
    </UiModal>
  );
}

