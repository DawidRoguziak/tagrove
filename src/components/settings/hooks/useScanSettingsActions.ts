import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  addScanRoot,
  cancelRenderAllThumbnails,
  listScanRoots,
  removeScanRoot,
  renderAllThumbnails,
  renderFailedThumbnails,
  getVideoToolStatus,
  scanFolder,
  rescanAllRoots
} from "../../../api";
import {
  formatThumbnailSummary,
  isScanProgressPhase,
  isThumbnailProgressPhase
} from "../services/progressService";
import type { ScanSummary } from "../../../types";
import type { ScanSettingsController } from "../types";
import type { useSettingsOperationRunner } from "./useSettingsOperationRunner";

function normalizeFolderSelection(selection: string | string[] | null): string[] | null {
  if (typeof selection === "string") {
    return [selection];
  }

  if (Array.isArray(selection)) {
    const uniqueSelection = [...new Set(selection)];
    return uniqueSelection.length ? uniqueSelection : null;
  }

  return null;
}

function formatFolderAddMessage(
  t: ReturnType<typeof useTranslation>["t"],
  selectedPaths: string[],
  addedPaths: string[],
  skippedCount: number
) {
  if (addedPaths.length === 1 && skippedCount === 0) {
    const path = selectedPaths.length === 1 ? selectedPaths[0] : addedPaths[0];
    return t("settings.actions.summary.folderAdded", { path });
  }

  if (addedPaths.length === 1) {
    return t("settings.actions.summary.folderAddedWithSkipped", {
      path: addedPaths[0],
      skipped: skippedCount
    });
  }

  if (addedPaths.length > 1 && skippedCount === 0) {
    return t("settings.actions.summary.foldersAdded", { count: addedPaths.length });
  }

  if (addedPaths.length > 1) {
    return t("settings.actions.summary.foldersAddedWithSkipped", {
      count: addedPaths.length,
      skipped: skippedCount
    });
  }

  if (skippedCount > 0) {
    return t("settings.actions.summary.noNewFoldersAddedWithSkipped", { skipped: skippedCount });
  }

  return t("settings.actions.summary.noNewFoldersAdded");
}

function formatScanSummary(t: ReturnType<typeof useTranslation>["t"], summary: ScanSummary) {
  return t(
    summary.completion === "partial"
      ? "settings.actions.summary.doneIndexedPartial"
      : "settings.actions.summary.doneIndexed",
    {
      indexed: summary.indexed,
      removed: summary.removed,
      failed: summary.failed
    }
  );
}

interface UseScanSettingsActionsOptions {
  runner: ReturnType<typeof useSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  onRootRemoved: () => void;
}

export function useScanSettingsActions({
  runner,
  refreshLibrary,
  onRootRemoved
}: UseScanSettingsActionsOptions): ScanSettingsController {
  const { t } = useTranslation();
  const [scanRoots, setScanRoots] = useState<string[]>([]);
  const [thumbnailBulkRunning, setThumbnailBulkRunning] = useState(false);
  const [cancelThumbnailRunning, setCancelThumbnailRunning] = useState(false);
  const [removeRootConfirmPath, setRemoveRootConfirmPath] = useState<string | null>(null);
  const [videoToolStatus, setVideoToolStatus] = useState<ScanSettingsController["videoToolStatus"]>(null);

  useEffect(() => {
    void getVideoToolStatus().then(setVideoToolStatus).catch(() => {
      setVideoToolStatus({ ffmpeg_available: false, ffprobe_available: false });
    });
  }, []);

  const refreshScanRoots = useCallback(async () => {
    const roots = await listScanRoots();
    setScanRoots(roots);
    return roots;
  }, []);

  const onPickFolder = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.waitFolderSelection"),
        errorPrefix: t("settings.actions.errorPrefix.addFolder"),
        setGlobalLoading: false
      },
      async () => {
        const selection = normalizeFolderSelection(
          await open({
            directory: true,
            multiple: true,
            title: t("settings.actions.dialogs.selectMediaFolder")
          })
        );

        if (!selection) {
          runner.setSectionMessage("scan", t("settings.actions.summary.folderSelectionCancelled"));
          return;
        }

        const previousRoots = await listScanRoots();
        const previousRootSet = new Set(previousRoots);

        for (const path of selection) {
          await addScanRoot(path);
        }

        const nextRoots = await refreshScanRoots();
        const addedPaths = nextRoots.filter((path) => !previousRootSet.has(path));
        const skippedCount = Math.max(selection.length - addedPaths.length, 0);

        runner.setSectionMessage(
          "scan",
          formatFolderAddMessage(t, selection, addedPaths, skippedCount)
        );
      }
    );
  }, [refreshScanRoots, runner, t]);

  const onRequestRemoveRoot = useCallback(
    (path: string) => {
      if (runner.isOperationLocked) {
        return;
      }

      setRemoveRootConfirmPath(path);
    },
    [runner.isOperationLocked]
  );

  const onCancelRemoveRootConfirm = useCallback(() => {
    if (runner.isOperationLocked) {
      return;
    }

    setRemoveRootConfirmPath(null);
  }, [runner.isOperationLocked]);

  const onConfirmRemoveRoot = useCallback(async () => {
    const path = removeRootConfirmPath;
    if (!path) {
      return;
    }

    setRemoveRootConfirmPath(null);

    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.removeFolder", { path }),
        errorPrefix: t("settings.actions.errorPrefix.removeFolder")
      },
      async () => {
        const summary = await removeScanRoot(path);
        onRootRemoved();
        await refreshScanRoots();
        await refreshLibrary();
        runner.setSectionMessage(
          "scan",
          t("settings.actions.summary.removedRoot", {
            assets: summary.removed_assets,
            thumbnails: summary.removed_thumbnails
          })
        );
      }
    );
  }, [onRootRemoved, refreshLibrary, refreshScanRoots, removeRootConfirmPath, runner, t]);

  const onRescanAll = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.rescanAll"),
        errorPrefix: t("settings.actions.errorPrefix.rescan"),
        progressPhaseMatcher: isScanProgressPhase
      },
      async () => {
        const summary = await rescanAllRoots();
        await refreshLibrary();
        await refreshScanRoots();
        runner.setSectionMessage("scan", formatScanSummary(t, summary));
      }
    );
  }, [refreshLibrary, refreshScanRoots, runner, t]);

  const onRescanRoot = useCallback(
    async (path: string) => {
      await runner.runExclusiveOperation(
        {
          section: "scan",
          pendingMessage: t("settings.actions.pending.rescanFolder", { path }),
          errorPrefix: t("settings.actions.errorPrefix.rescanFolder"),
          progressPhaseMatcher: isScanProgressPhase
        },
        async () => {
          const summary = await scanFolder(path);
          await refreshLibrary();
          await refreshScanRoots();
          runner.setSectionMessage("scan", formatScanSummary(t, summary));
        }
      );
    },
    [refreshLibrary, refreshScanRoots, runner, t]
  );

  const onRenderAllThumbnails = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.renderAllThumbs"),
        errorPrefix: t("settings.actions.errorPrefix.renderThumbs"),
        progressPhaseMatcher: isThumbnailProgressPhase
      },
      async () => {
        setThumbnailBulkRunning(true);
        try {
          const summary = await renderAllThumbnails();
          await refreshLibrary();
          runner.setSectionMessage(
            "scan",
            formatThumbnailSummary(t("settings.actions.summary.allThumbnailsLabel"), summary)
          );
        } finally {
          setThumbnailBulkRunning(false);
        }
      }
    );
  }, [refreshLibrary, runner, t]);

  const onRenderFailedThumbnails = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.retryFailedThumbs"),
        errorPrefix: t("settings.actions.errorPrefix.retryThumbs"),
        progressPhaseMatcher: isThumbnailProgressPhase
      },
      async () => {
        setThumbnailBulkRunning(true);
        try {
          const summary = await renderFailedThumbnails();
          await refreshLibrary();
          runner.setSectionMessage(
            "scan",
            formatThumbnailSummary(t("settings.actions.summary.failedRetryLabel"), summary)
          );
        } finally {
          setThumbnailBulkRunning(false);
        }
      }
    );
  }, [refreshLibrary, runner, t]);

  const onCancelThumbnailRender = useCallback(async () => {
    if (cancelThumbnailRunning) {
      return;
    }

    setCancelThumbnailRunning(true);
    try {
      const accepted = await cancelRenderAllThumbnails();
      if (accepted) {
        runner.setSectionMessage("scan", t("settings.actions.summary.stopRequested"));
      } else {
        runner.setSectionMessage("scan", t("settings.actions.summary.noThumbRenderRunning"));
      }
    } catch (error) {
      runner.setSectionMessage(
        "scan",
        `${t("settings.actions.errorPrefix.cancelThumbRender")}: ${String(error)}`
      );
    } finally {
      setCancelThumbnailRunning(false);
    }
  }, [cancelThumbnailRunning, runner, t]);

  return useMemo(
    () => ({
      scanRoots,
      isOperationLocked: runner.isOperationLocked,
      thumbnailBulkRunning,
      cancelThumbnailRunning,
      operationState: runner.scanOperationState,
      videoToolStatus,
      removeRootConfirmPath,
      refreshScanRoots,
      onPickFolder,
      onRescanRoot,
      onRequestRemoveRoot,
      onRescanAll,
      onRenderAllThumbnails,
      onRenderFailedThumbnails,
      onCancelThumbnailRender,
      onCancelRemoveRootConfirm,
      onConfirmRemoveRoot
    }),
    [
      cancelThumbnailRunning,
      onCancelRemoveRootConfirm,
      onCancelThumbnailRender,
      onConfirmRemoveRoot,
      onPickFolder,
      onRenderAllThumbnails,
      onRenderFailedThumbnails,
      onRequestRemoveRoot,
      onRescanAll,
      onRescanRoot,
      refreshScanRoots,
      removeRootConfirmPath,
      runner.isOperationLocked,
      runner.scanOperationState,
      scanRoots,
      thumbnailBulkRunning,
      videoToolStatus
    ]
  );
}
