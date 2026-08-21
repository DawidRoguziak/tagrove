import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { applyDuplicateResolutionBatch, findDuplicateAssets } from "../../../api";
import type { DuplicateGroup, DuplicateResolutionBatchChange } from "../../../types";
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
    revision: number;
  },
  setGroups: (groups: DuplicateGroup[]) => void,
  setAssetCount: (count: number) => void,
  setRevision: (revision: number) => void
) {
  setGroups(summary.groups);
  setAssetCount(summary.duplicate_assets);
  setRevision(summary.revision);
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
  const [scanRevision, setScanRevision] = useState(0);

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
        applyDuplicateScanResult(summary, setGroups, setAssetCount, setScanRevision);
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
          const assetsById = new Map(groups.flatMap((group) => group.assets).map((asset) => [asset.id, asset]));
          const batchChanges: DuplicateResolutionBatchChange[] = changes.map((change) => {
            const asset = assetsById.get(change.assetId);
            if (!asset) throw new Error(t("validation.unknownAssetInChanges", { assetId: change.assetId }));
            const common = {
              assetId: asset.id,
              expectedPath: asset.path,
              expectedRecordVersion: asset.record_version
            };
            if (change.type === "rename") {
              const newFileName = change.nextFileName.trim();
              if (!newFileName) throw new Error(t("validation.fileNameCannotBeEmpty"));
              return { type: "rename", ...common, newFileName };
            }
            return { type: "delete", ...common };
          });
          const result = await applyDuplicateResolutionBatch(scanRevision, batchChanges);
          await refreshLibrary().catch(() => undefined);
          const duplicateSummary = await findDuplicateAssets().catch(() => null);
          if (duplicateSummary) {
            applyDuplicateScanResult(duplicateSummary, setGroups, setAssetCount, setScanRevision);
          }
          if (result.status === "rolled_back") {
            runner.setSectionMessage("duplicates", t("settings.actions.summary.duplicateRolledBack"));
            return;
          }
          if (result.status === "recovery_required") {
            const recoveryItems = result.results.filter((item) => item.recovery_path);
            const details = recoveryItems
              .map((item) => `#${item.asset_id}: ${item.recovery_path}`)
              .join("; ");
            runner.setSectionMessage(
              "duplicates",
              `${t("settings.actions.summary.duplicateRecoveryRequired", {
                count: recoveryItems.length
              })} ${details}`.trim()
            );
            return;
          }
          const missingSources = result.results.filter(
            (item) => item.status === "source_missing"
          ).length;
          const appliedMessage = t("settings.actions.summary.duplicateApplied", {
            count: changes.length,
            groups: duplicateSummary?.duplicate_groups ?? groups.length
          });
          runner.setSectionMessage(
            "duplicates",
            missingSources > 0
              ? `${appliedMessage} ${t("lightbox.deleteConfirm.sourceMissing")} (${missingSources})`
              : appliedMessage
          );
        }
      );
    },
    [groups, refreshLibrary, runner, scanRevision, t]
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
