import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { clearLibraryData } from "../../../api";
import { isLibraryClearProgressPhase } from "../services/progressService";
import type { DangerZoneSettingsController } from "../types";
import type { useSettingsOperationRunner } from "./useSettingsOperationRunner";

interface UseDangerZoneSettingsActionsOptions {
  runner: ReturnType<typeof useSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  refreshScanRoots: () => Promise<string[]>;
  onLibraryCleared: () => void;
  runWithTagMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
}

export function useDangerZoneSettingsActions({
  runner,
  refreshLibrary,
  refreshScanRoots,
  onLibraryCleared,
  runWithTagMutationBarrier
}: UseDangerZoneSettingsActionsOptions): DangerZoneSettingsController {
  const { t } = useTranslation();
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);

  const handleClearLibraryConfirmed = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "danger",
        pendingMessage: t("settings.actions.pending.clearLibrary"),
        errorPrefix: t("settings.actions.errorPrefix.clearLibrary"),
        progressPhaseMatcher: isLibraryClearProgressPhase
      },
      async () => {
        let summary: Awaited<ReturnType<typeof clearLibraryData>>;
        try {
          summary = await runWithTagMutationBarrier(async () => {
            try {
              return await clearLibraryData();
            } finally {
              // Clear can reject after partial commits. Invalidate identity-bound state before
              // maintenance ownership is released, regardless of the command result.
              onLibraryCleared();
            }
          });
        } catch (error) {
          await Promise.allSettled([refreshScanRoots(), refreshLibrary()]);
          throw error;
        }

        await refreshScanRoots();
        runner.setSectionMessage(
          "danger",
          t("settings.actions.summary.libraryCleared", {
            assets: summary.removed_assets,
            roots: summary.removed_roots,
            thumbnails: summary.removed_thumbnails
          })
        );
      }
    );
  }, [
    onLibraryCleared,
    refreshLibrary,
    refreshScanRoots,
    runWithTagMutationBarrier,
    runner,
    t
  ]);

  const onOpenClearConfirm = useCallback(() => {
    setClearConfirmOpen(true);
  }, []);

  const onCloseClearConfirm = useCallback(() => {
    setClearConfirmOpen(false);
  }, []);

  const onConfirmClearYes = useCallback(async () => {
    setClearConfirmOpen(false);
    await handleClearLibraryConfirmed();
  }, [handleClearLibraryConfirmed]);

  return useMemo(
    () => ({
      isOperationLocked: runner.isOperationLocked,
      operationState: runner.dangerOperationState,
      clearConfirmOpen,
      onOpenClearConfirm,
      onCloseClearConfirm,
      onConfirmClearYes
    }),
    [
      clearConfirmOpen,
      onCloseClearConfirm,
      onConfirmClearYes,
      onOpenClearConfirm,
      runner.dangerOperationState,
      runner.isOperationLocked
    ]
  );
}
