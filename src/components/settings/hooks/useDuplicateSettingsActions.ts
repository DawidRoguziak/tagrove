import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { deleteAsset, findDuplicateAssets, renameAssetFile } from "../../../api";
import type { DuplicateGroup } from "../../../types";
import { isDuplicateScanProgressPhase } from "../services/progressService";
import type {
  DuplicatesSettingsController,
  DuplicateResolutionChange,
  PendingDuplicateDeleteConfirm
} from "../types";
import type { useSettingsOperationRunner } from "./useSettingsOperationRunner";

interface UseDuplicateSettingsActionsOptions {
  runner: ReturnType<typeof useSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  thumbs?: Record<number, string>;
  renderingThumbnailIds?: Record<number, true>;
  onQueueThumbnailsByIds?: (assetIds: number[]) => void;
}

function applyDuplicateScanResult(
  summary: {
    groups: DuplicateGroup[];
    duplicate_assets: number;
  },
  setGroups: (groups: DuplicateGroup[]) => void,
  setAssetCount: (count: number) => void
) {
  setGroups(summary.groups);
  setAssetCount(summary.duplicate_assets);
}

export function useDuplicateSettingsActions({
  runner,
  refreshLibrary,
  thumbs,
  renderingThumbnailIds,
  onQueueThumbnailsByIds
}: UseDuplicateSettingsActionsOptions): DuplicatesSettingsController {
  const { t } = useTranslation();
  const [duplicateDialogOpen, setDuplicateDialogOpen] = useState(false);
  const [pendingDuplicateDeleteConfirm, setPendingDuplicateDeleteConfirm] =
    useState<PendingDuplicateDeleteConfirm | null>(null);
  const [groups, setGroups] = useState<DuplicateGroup[]>([]);
  const [assetCount, setAssetCount] = useState(0);

  const onRescan = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "duplicates",
        pendingMessage: t("settings.actions.pending.findDuplicates"),
        errorPrefix: t("settings.actions.errorPrefix.duplicateScan"),
        progressPhaseMatcher: isDuplicateScanProgressPhase,
        setGlobalLoading: false
      },
      async () => {
        const summary = await findDuplicateAssets();
        applyDuplicateScanResult(summary, setGroups, setAssetCount);
        runner.setSectionMessage(
          "duplicates",
          t("settings.actions.summary.duplicateScanComplete", {
            groups: summary.duplicate_groups,
            assets: summary.duplicate_assets
          })
        );
      }
    );
  }, [runner, t]);

  const onStart = useCallback(async () => {
    setDuplicateDialogOpen(true);
    await onRescan();
  }, [onRescan]);

  const runSaveDuplicateChanges = useCallback(
    async (changes: DuplicateResolutionChange[]) => {
      await runner.runExclusiveOperation(
        {
          section: "duplicates",
          pendingMessage: t("settings.actions.pending.applyDuplicateChanges"),
          errorPrefix: t("settings.actions.errorPrefix.saveDuplicateChanges"),
          setGlobalLoading: false
        },
        async () => {
          for (const change of changes) {
            if (change.type === "rename") {
              const trimmedName = change.nextFileName.trim();
              if (!trimmedName) {
                throw new Error(t("validation.fileNameCannotBeEmpty"));
              }
              await renameAssetFile(change.assetId, trimmedName);
              continue;
            }

            await deleteAsset(change.assetId);
          }

          await refreshLibrary();
          const duplicateSummary = await findDuplicateAssets();
          applyDuplicateScanResult(duplicateSummary, setGroups, setAssetCount);
          runner.setSectionMessage(
            "duplicates",
            t("settings.actions.summary.duplicateApplied", {
              count: changes.length,
              groups: duplicateSummary.duplicate_groups
            })
          );
        }
      );
    },
    [refreshLibrary, runner, t]
  );

  const onSaveAll = useCallback(
    async (changes: DuplicateResolutionChange[]) => {
      if (!changes.length) {
        runner.setSectionMessage("duplicates", t("settings.actions.summary.noDuplicateChanges"));
        return;
      }

      const deleteCount = changes.filter((change) => change.type === "delete").length;
      if (deleteCount > 0) {
        setPendingDuplicateDeleteConfirm({
          changes,
          deleteCount
        });
        return;
      }

      await runSaveDuplicateChanges(changes);
    },
    [runSaveDuplicateChanges, runner, t]
  );

  const onCancelDuplicateDeleteConfirm = useCallback(() => {
    if (runner.isOperationLocked) {
      return;
    }

    setPendingDuplicateDeleteConfirm(null);
    runner.setSectionMessage("duplicates", t("settings.actions.summary.duplicateApplyCancelled"));
  }, [runner, t]);

  const onConfirmDuplicateDeleteConfirm = useCallback(async () => {
    const pending = pendingDuplicateDeleteConfirm;
    if (!pending) {
      return;
    }

    setPendingDuplicateDeleteConfirm(null);
    await runSaveDuplicateChanges(pending.changes);
  }, [pendingDuplicateDeleteConfirm, runSaveDuplicateChanges]);

  const onCloseDuplicateDialog = useCallback(() => {
    setDuplicateDialogOpen(false);
  }, []);

  return useMemo(
    () => ({
      isOperationLocked: runner.isOperationLocked,
      operationState: runner.duplicateOperationState,
      duplicateDialogOpen,
      pendingDuplicateDeleteConfirm,
      groups,
      assetCount,
      thumbs,
      renderingThumbnailIds,
      onQueueThumbnailsByIds,
      onStart,
      onCloseDuplicateDialog,
      onRescan,
      onSaveAll,
      onCancelDuplicateDeleteConfirm,
      onConfirmDuplicateDeleteConfirm
    }),
    [
      duplicateDialogOpen,
      groups,
      onCancelDuplicateDeleteConfirm,
      onCloseDuplicateDialog,
      onConfirmDuplicateDeleteConfirm,
      onQueueThumbnailsByIds,
      onRescan,
      onSaveAll,
      onStart,
      pendingDuplicateDeleteConfirm,
      renderingThumbnailIds,
      runner.duplicateOperationState,
      runner.isOperationLocked,
      thumbs,
      assetCount
    ]
  );
}
