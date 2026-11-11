import type { ScanRoot } from "../../platform/tauri/contracts/scanning";
import { createSignal } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import { clearLibraryData } from "../../platform/tauri/maintenance";
import { isLibraryClearProgressPhase } from "./progressService";
import type { DangerZoneSettingsController } from "./types";
import type { createSettingsOperationRunner } from "./createSettingsOperationRunner";
interface Options {
  runner: ReturnType<typeof createSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  refreshScanRoots: () => Promise<ScanRoot[]>;
  onLibraryCleared: () => void;
  runWithTagMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
}
export function createDangerZoneSettings({
  runner,
  refreshLibrary,
  refreshScanRoots,
  onLibraryCleared,
  runWithTagMutationBarrier
}: Options): DangerZoneSettingsController {
  const { t } = useLanguage();
  const [clearConfirmOpen, setClearConfirmOpen] = createSignal(false);
  const handleClearLibraryConfirmed = async () => {
    await runner.runExclusiveOperation(
      {
        section: "danger",
        pendingMessage: () => t("settings.actions.pending.clearLibrary"),
        errorPrefix: () => t("settings.actions.errorPrefix.clearLibrary"),
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
        runner.setSectionMessage("danger", () =>
          t("settings.actions.summary.libraryCleared", {
            assets: summary.removed_assets,
            roots: summary.removed_roots,
            thumbnails: summary.removed_thumbnails
          })
        );
      }
    );
  };
  const onOpenClearConfirm = () => {
    setClearConfirmOpen(true);
  };
  const onCloseClearConfirm = () => {
    setClearConfirmOpen(false);
  };
  const onConfirmClearYes = async () => {
    setClearConfirmOpen(false);
    await handleClearLibraryConfirmed();
  };
  return {
    get isOperationLocked() {
      return runner.isOperationLocked;
    },
    get operationState() {
      return runner.dangerOperationState;
    },
    get clearConfirmOpen() {
      return clearConfirmOpen();
    },
    onOpenClearConfirm,
    onCloseClearConfirm,
    onConfirmClearYes
  };
}
