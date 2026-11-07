import {
  cancelRenderAllThumbnails,
  renderAllThumbnails,
  renderFailedThumbnails
} from "../../platform/tauri/thumbnails";
import { getVideoToolStatus } from "../../platform/tauri/maintenance";
import { createSignal, onCleanup, onSettled } from "solid-js";
import { open } from "../../platform/tauri/dialogs";
import { useLanguage } from "../../i18n/solid";
import {
  addScanRoot,
  setScanRootAutoScan,
  scanStartupRoots,
  listScanRoots,
  removeScanRoot,
  scanFolder,
  rescanAllRoots
} from "../../platform/tauri/scanning";
import {
  formatThumbnailSummary,
  isScanProgressPhase,
  isThumbnailProgressPhase
} from "./progressService";
import type { ScanRoot, ScanSummary } from "../../platform/tauri/contracts/scanning";
import type { ScanSettingsController } from "./types";
import type { createSettingsOperationRunner } from "./createSettingsOperationRunner";
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
  t: ReturnType<typeof useLanguage>["t"],
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
function formatScanSummary(t: ReturnType<typeof useLanguage>["t"], summary: ScanSummary) {
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
interface Options {
  runner: ReturnType<typeof createSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  onRootRemoved?: () => void;
}
export function createScanSettings({
  runner,
  refreshLibrary,
  onRootRemoved
}: Options): ScanSettingsController {
  const { t } = useLanguage();
  const [scanRoots, setScanRoots] = createSignal<ScanRoot[]>([]);
  const [thumbnailBulkRunning, setThumbnailBulkRunning] = createSignal(false);
  const [cancelThumbnailRunning, setCancelThumbnailRunning] = createSignal(false);
  // Stop replies belong to one render, even after its runner lock is released.
  let activeThumbnailRender: symbol | null = null;
  let cancelPending: symbol | null = null;
  const [removeRootConfirmPath, publishRemoveRootConfirmPath] = createSignal<string | null>(null);
  let pendingRoot: string | null = null;
  function setRemoveRootConfirmPath(path: string | null) {
    pendingRoot = path;
    publishRemoveRootConfirmPath(path);
  }
  const [videoToolStatus, setVideoToolStatus] =
    createSignal<ScanSettingsController["videoToolStatus"]>(null);
  let disposed = false;
  let rootsGeneration = 0;
  onCleanup(() => {
    disposed = true;
  });
  onSettled(() => {
    void getVideoToolStatus()
      .then((status) => {
        if (!disposed) setVideoToolStatus(status);
      })
      .catch(() => {
        if (!disposed) setVideoToolStatus({ ffmpeg_available: false, ffprobe_available: false });
      });
  });
  const refreshScanRoots = async () => {
    const generation = ++rootsGeneration;
    const roots = await listScanRoots();
    if (!disposed && generation === rootsGeneration) setScanRoots(roots);
    return roots;
  };
  let startupRequested = false;
  const scanOnStartup = async () => {
    if (startupRequested) return;
    startupRequested = true;
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.scan.startupPending"),
        errorPrefix: t("settings.actions.errorPrefix.rescan"),
        progressPhaseMatcher: isScanProgressPhase
      },
      async () => {
        let summary: ScanSummary | null;
        try {
          summary = await scanStartupRoots();
        } catch (error) {
          if (disposed) return;
          await Promise.allSettled([refreshLibrary(), refreshScanRoots()]);
          throw error;
        }
        if (disposed) return;
        if (summary) {
          await refreshLibrary();
          await refreshScanRoots();
          runner.setSectionMessage("scan", formatScanSummary(t, summary));
        } else {
          runner.setSectionMessage("scan", "");
        }
      }
    );
  };
  const onSetAutoScan = async (path: string, enabled: boolean) => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.scan.savingAutoScan"),
        errorPrefix: t("settings.scan.saveAutoScanError"),
        setGlobalLoading: false
      },
      async () => {
        await setScanRootAutoScan(path, enabled);
        setScanRoots((roots) =>
          roots.map((root) =>
            root.path === path ? { ...root, auto_scan_on_startup: enabled } : root
          )
        );
        runner.setSectionMessage("scan", t("settings.scan.autoScanSaved"));
      }
    );
  };
  const onPickFolder = async () => {
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
        const previousRootSet = new Set(previousRoots.map((root) => root.path));
        for (const path of selection) {
          await addScanRoot(path);
        }
        const nextRoots = await refreshScanRoots();
        const addedPaths = nextRoots
          .map((root) => root.path)
          .filter((path) => !previousRootSet.has(path));
        const skippedCount = Math.max(selection.length - addedPaths.length, 0);
        runner.setSectionMessage(
          "scan",
          formatFolderAddMessage(t, selection, addedPaths, skippedCount)
        );
      }
    );
  };
  const onRequestRemoveRoot = (path: string) => {
    if (runner.isOperationLocked) {
      return;
    }
    setRemoveRootConfirmPath(path);
  };
  const onCancelRemoveRootConfirm = () => {
    if (runner.isOperationLocked) {
      return;
    }
    setRemoveRootConfirmPath(null);
  };
  const onConfirmRemoveRoot = async () => {
    if (runner.isOperationLocked) return;
    const path = pendingRoot;
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
        onRootRemoved?.();
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
  };
  const onRescanAll = async () => {
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
  };
  const onRescanRoot = async (path: string) => {
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
  };
  const onRenderAllThumbnails = async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.renderAllThumbs"),
        errorPrefix: t("settings.actions.errorPrefix.renderThumbs"),
        progressPhaseMatcher: isThumbnailProgressPhase
      },
      async () => {
        activeThumbnailRender = Symbol();
        setThumbnailBulkRunning(true);
        try {
          const summary = await renderAllThumbnails();
          await refreshLibrary();
          runner.setSectionMessage(
            "scan",
            formatThumbnailSummary(t("settings.actions.summary.allThumbnailsLabel"), summary)
          );
        } finally {
          activeThumbnailRender = null;
          cancelPending = null;
          setCancelThumbnailRunning(false);
          setThumbnailBulkRunning(false);
        }
      }
    );
  };
  const onRenderFailedThumbnails = async () => {
    await runner.runExclusiveOperation(
      {
        section: "scan",
        pendingMessage: t("settings.actions.pending.retryFailedThumbs"),
        errorPrefix: t("settings.actions.errorPrefix.retryThumbs"),
        progressPhaseMatcher: isThumbnailProgressPhase
      },
      async () => {
        activeThumbnailRender = Symbol();
        setThumbnailBulkRunning(true);
        try {
          const summary = await renderFailedThumbnails();
          await refreshLibrary();
          runner.setSectionMessage(
            "scan",
            formatThumbnailSummary(t("settings.actions.summary.failedRetryLabel"), summary)
          );
        } finally {
          activeThumbnailRender = null;
          cancelPending = null;
          setCancelThumbnailRunning(false);
          setThumbnailBulkRunning(false);
        }
      }
    );
  };
  const onCancelThumbnailRender = async () => {
    const render = activeThumbnailRender;
    if (!render || cancelPending || disposed) {
      return;
    }
    cancelPending = render;
    setCancelThumbnailRunning(true);
    try {
      const accepted = await cancelRenderAllThumbnails();
      if (disposed || activeThumbnailRender !== render) return;
      if (accepted) {
        runner.setSectionMessage("scan", t("settings.actions.summary.stopRequested"));
      } else {
        runner.setSectionMessage("scan", t("settings.actions.summary.noThumbRenderRunning"));
      }
    } catch (error) {
      if (disposed || activeThumbnailRender !== render) return;
      runner.setSectionMessage(
        "scan",
        `${t("settings.actions.errorPrefix.cancelThumbRender")}: ${String(error)}`
      );
    } finally {
      if (cancelPending === render) {
        cancelPending = null;
        setCancelThumbnailRunning(false);
      }
    }
  };
  return {
    get scanRoots() {
      return scanRoots();
    },
    scanOnStartup,
    onSetAutoScan,
    get isOperationLocked() {
      return runner.isOperationLocked;
    },
    get thumbnailBulkRunning() {
      return thumbnailBulkRunning();
    },
    get cancelThumbnailRunning() {
      return cancelThumbnailRunning();
    },
    get operationState() {
      return runner.scanOperationState;
    },
    get videoToolStatus() {
      return videoToolStatus();
    },
    get removeRootConfirmPath() {
      return removeRootConfirmPath();
    },
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
  };
}
