import { useMemo } from "react";
import { useDangerZoneSettingsActions } from "../components/settings/hooks/useDangerZoneSettingsActions";
import { useDuplicateSettingsActions } from "../components/settings/hooks/useDuplicateSettingsActions";
import { useImportExportSettingsActions } from "../components/settings/hooks/useImportExportSettingsActions";
import { useScanSettingsActions } from "../components/settings/hooks/useScanSettingsActions";
import { useSettingsOperationRunner } from "../components/settings/hooks/useSettingsOperationRunner";

async function runWithoutTagMutationBarrier<T>(operation: () => Promise<T>): Promise<T> {
  return operation();
}

interface UseSettingsActionsOptions {
  setLoading: (value: boolean) => void;
  refreshLibrary: () => Promise<void>;
  onRootRemoved: () => void;
  onImportDbRestored: () => void;
  onLibraryCleared: () => void;
  onTagCacheInvalidated?: () => void;
  runWithTagMutationBarrier?: <T>(operation: () => Promise<T>) => Promise<T>;
  duplicateThumbs?: Record<number, string>;
  duplicateRenderingThumbnailIds?: Record<number, true>;
  onQueueDuplicateThumbnailsByIds?: (assetIds: number[]) => void;
}

export function useSettingsActions({
  setLoading,
  refreshLibrary,
  onRootRemoved,
  onImportDbRestored,
  onLibraryCleared,
  onTagCacheInvalidated = () => {},
  runWithTagMutationBarrier = runWithoutTagMutationBarrier,
  duplicateThumbs,
  duplicateRenderingThumbnailIds,
  onQueueDuplicateThumbnailsByIds
}: UseSettingsActionsOptions) {
  const runner = useSettingsOperationRunner({ setLoading });

  const scan = useScanSettingsActions({
    runner,
    refreshLibrary,
    onRootRemoved
  });

  const importExport = useImportExportSettingsActions({
    runner,
    refreshLibrary,
    refreshScanRoots: scan.refreshScanRoots,
    onImportDbRestored,
    onTagCacheInvalidated,
    runWithTagMutationBarrier
  });

  const dangerZone = useDangerZoneSettingsActions({
    runner,
    refreshLibrary,
    refreshScanRoots: scan.refreshScanRoots,
    onLibraryCleared,
    runWithTagMutationBarrier
  });

  const duplicates = useDuplicateSettingsActions({
    runner,
    refreshLibrary,
    thumbs: duplicateThumbs,
    renderingThumbnailIds: duplicateRenderingThumbnailIds,
    onQueueThumbnailsByIds: onQueueDuplicateThumbnailsByIds
  });

  return useMemo(
    () => ({
      scan,
      importExport,
      dangerZone,
      duplicates,
      scanRoots: scan.scanRoots,
      thumbnailBulkRunning: scan.thumbnailBulkRunning,
      cancelThumbnailRunning: scan.cancelThumbnailRunning,
      isOperationLocked: scan.isOperationLocked,
      removeRootConfirmPath: scan.removeRootConfirmPath,
      clearConfirmOpen: dangerZone.clearConfirmOpen,
      duplicateDialogOpen: duplicates.duplicateDialogOpen,
      pendingDuplicateDeleteConfirm: duplicates.pendingDuplicateDeleteConfirm,
      pendingImportDbSourcePath: importExport.pendingImportDbSourcePath,
      duplicateGroups: duplicates.groups,
      duplicateAssetCount: duplicates.assetCount,
      scanOperationState: scan.operationState,
      importExportOperationState: importExport.operationState,
      dangerOperationState: dangerZone.operationState,
      duplicateOperationState: duplicates.operationState,
      refreshScanRoots: scan.refreshScanRoots,
      pickFolder: scan.onPickFolder,
      requestRemoveRoot: scan.onRequestRemoveRoot,
      cancelRemoveRoot: scan.onCancelRemoveRootConfirm,
      confirmRemoveRoot: scan.onConfirmRemoveRoot,
      handleRescanRoot: scan.onRescanRoot,
      handleRescanAll: scan.onRescanAll,
      handleRenderAllThumbnails: scan.onRenderAllThumbnails,
      handleRenderFailedThumbnails: scan.onRenderFailedThumbnails,
      handleCancelThumbnailRender: scan.onCancelThumbnailRender,
      handleExportCsv: importExport.onExportCsv,
      handleImportCsv: importExport.onImportCsv,
      handleExportDbBundle: importExport.onExportDbBundle,
      handleImportDbBundle: importExport.onImportDbBundle,
      setClearConfirmOpen: (value: boolean) => {
        if (value) {
          dangerZone.onOpenClearConfirm();
          return;
        }

        dangerZone.onCloseClearConfirm();
      },
      setDuplicateDialogOpen: (value: boolean) => {
        if (!value) {
          duplicates.onCloseDuplicateDialog();
        }
      },
      handleConfirmClearYes: dangerZone.onConfirmClearYes,
      handleStartDuplicateScan: duplicates.onStart,
      handleRescanDuplicates: duplicates.onRescan,
      handleSaveDuplicateChanges: duplicates.onSaveAll,
      cancelPendingDuplicateDeleteConfirm: duplicates.onCancelDuplicateDeleteConfirm,
      confirmPendingDuplicateDeleteConfirm: duplicates.onConfirmDuplicateDeleteConfirm,
      cancelPendingImportDbOverwriteConfirm: importExport.onCancelImportDbOverwriteConfirm,
      confirmPendingImportDbOverwriteConfirm: importExport.onConfirmImportDbOverwriteConfirm
    }),
    [dangerZone, duplicates, importExport, scan]
  );
}
