import { ClearLibraryConfirmDialog } from "./ClearLibraryConfirmDialog";
import { DuplicateDeleteConfirmDialog } from "./DuplicateDeleteConfirmDialog";
import { DuplicateResolverDialog } from "./DuplicateResolverDialog";
import { ImportDbOverwriteConfirmDialog } from "./ImportDbOverwriteConfirmDialog";
import { RemoveScanRootConfirmDialog } from "./RemoveScanRootConfirmDialog";
import { AppearanceSection } from "./sections/AppearanceSection";
import { DangerZoneSection } from "./sections/DangerZoneSection";
import { DuplicatesSection } from "./sections/DuplicatesSection";
import { ImportExportSection } from "./sections/ImportExportSection";
import { ScanSettingsSection } from "./sections/ScanSettingsSection";
import type {
  DangerZoneSettingsController,
  DuplicatesSettingsController,
  ImportExportSettingsController,
  ScanSettingsController,
  SettingsAppearanceController
} from "./types";

export interface SettingsPanelProps {
  fullView?: boolean;
  highlightScanSection?: boolean;
  appearance?: SettingsAppearanceController;
  scan?: ScanSettingsController;
  importExport?: ImportExportSettingsController;
  dangerZone?: DangerZoneSettingsController;
  duplicates?: DuplicatesSettingsController;
  theme?: SettingsAppearanceController["theme"];
  onThemeChange?: SettingsAppearanceController["onThemeChange"];
  language?: SettingsAppearanceController["language"];
  onLanguageChange?: SettingsAppearanceController["onLanguageChange"];
  scanRoots?: ScanSettingsController["scanRoots"];
  isOperationLocked?: boolean;
  thumbnailBulkRunning?: boolean;
  cancelThumbnailRunning?: boolean;
  scanOperationState?: ScanSettingsController["operationState"];
  importExportOperationState?: ImportExportSettingsController["operationState"];
  dangerOperationState?: DangerZoneSettingsController["operationState"];
  duplicateOperationState?: DuplicatesSettingsController["operationState"];
  duplicateGroups?: DuplicatesSettingsController["groups"];
  duplicateAssetCount?: DuplicatesSettingsController["assetCount"];
  duplicateThumbs?: DuplicatesSettingsController["thumbs"];
  duplicateRenderingThumbnailIds?: DuplicatesSettingsController["renderingThumbnailIds"];
  onQueueDuplicateThumbnailsByIds?: DuplicatesSettingsController["onQueueThumbnailsByIds"];
  removeRootConfirmPath?: ScanSettingsController["removeRootConfirmPath"];
  clearConfirmOpen?: DangerZoneSettingsController["clearConfirmOpen"];
  duplicateDialogOpen?: DuplicatesSettingsController["duplicateDialogOpen"];
  pendingDuplicateDeleteConfirm?: DuplicatesSettingsController["pendingDuplicateDeleteConfirm"];
  pendingImportDbSourcePath?: ImportExportSettingsController["pendingImportDbSourcePath"];
  onPickFolder?: ScanSettingsController["onPickFolder"];
  onRescanRoot?: ScanSettingsController["onRescanRoot"];
  onRemoveRoot?: ScanSettingsController["onRequestRemoveRoot"];
  onRescanAll?: ScanSettingsController["onRescanAll"];
  onRenderAllThumbnails?: ScanSettingsController["onRenderAllThumbnails"];
  onRenderFailedThumbnails?: ScanSettingsController["onRenderFailedThumbnails"];
  onCancelThumbnailRender?: ScanSettingsController["onCancelThumbnailRender"];
  onExportCsv?: ImportExportSettingsController["onExportCsv"];
  onImportCsv?: ImportExportSettingsController["onImportCsv"];
  onExportDbBundle?: ImportExportSettingsController["onExportDbBundle"];
  onImportDbBundle?: ImportExportSettingsController["onImportDbBundle"];
  onCancelRemoveRootConfirm?: ScanSettingsController["onCancelRemoveRootConfirm"];
  onConfirmRemoveRoot?: ScanSettingsController["onConfirmRemoveRoot"];
  onOpenClearConfirm?: DangerZoneSettingsController["onOpenClearConfirm"];
  onCloseClearConfirm?: DangerZoneSettingsController["onCloseClearConfirm"];
  onConfirmClearYes?: DangerZoneSettingsController["onConfirmClearYes"];
  onStartDuplicateScan?: DuplicatesSettingsController["onStart"];
  onCloseDuplicateDialog?: DuplicatesSettingsController["onCloseDuplicateDialog"];
  onRescanDuplicates?: DuplicatesSettingsController["onRescan"];
  onSaveDuplicateChanges?: DuplicatesSettingsController["onSaveAll"];
  onCancelDuplicateDeleteConfirm?: DuplicatesSettingsController["onCancelDuplicateDeleteConfirm"];
  onConfirmDuplicateDeleteConfirm?: DuplicatesSettingsController["onConfirmDuplicateDeleteConfirm"];
  onCancelImportDbOverwriteConfirm?: ImportExportSettingsController["onCancelImportDbOverwriteConfirm"];
  onConfirmImportDbOverwriteConfirm?: ImportExportSettingsController["onConfirmImportDbOverwriteConfirm"];
}

export function SettingsPanel({
  fullView = false,
  highlightScanSection = false,
  appearance,
  scan,
  importExport,
  dangerZone,
  duplicates,
  theme,
  onThemeChange,
  language,
  onLanguageChange,
  scanRoots,
  isOperationLocked = false,
  thumbnailBulkRunning = false,
  cancelThumbnailRunning = false,
  scanOperationState,
  importExportOperationState,
  dangerOperationState,
  duplicateOperationState,
  duplicateGroups = [],
  duplicateAssetCount = 0,
  duplicateThumbs,
  duplicateRenderingThumbnailIds,
  onQueueDuplicateThumbnailsByIds,
  removeRootConfirmPath = null,
  clearConfirmOpen = false,
  duplicateDialogOpen = false,
  pendingDuplicateDeleteConfirm = null,
  pendingImportDbSourcePath = null,
  onPickFolder,
  onRescanRoot,
  onRemoveRoot,
  onRescanAll,
  onRenderAllThumbnails,
  onRenderFailedThumbnails,
  onCancelThumbnailRender,
  onExportCsv,
  onImportCsv,
  onExportDbBundle,
  onImportDbBundle,
  onCancelRemoveRootConfirm,
  onConfirmRemoveRoot,
  onOpenClearConfirm,
  onCloseClearConfirm,
  onConfirmClearYes,
  onStartDuplicateScan,
  onCloseDuplicateDialog,
  onRescanDuplicates,
  onSaveDuplicateChanges,
  onCancelDuplicateDeleteConfirm,
  onConfirmDuplicateDeleteConfirm,
  onCancelImportDbOverwriteConfirm,
  onConfirmImportDbOverwriteConfirm
}: SettingsPanelProps) {
  const appearanceController =
    appearance ??
    ({
      theme: theme ?? "light",
      onThemeChange: onThemeChange ?? (() => {}),
      language: language ?? "en",
      onLanguageChange: onLanguageChange ?? (() => {})
    } satisfies SettingsAppearanceController);
  const scanController =
    scan ??
    ({
      scanRoots: scanRoots ?? [],
      isOperationLocked,
      thumbnailBulkRunning,
      cancelThumbnailRunning,
      operationState: scanOperationState ?? { loading: false, message: "", progress: null },
      removeRootConfirmPath,
      refreshScanRoots: async () => scanRoots ?? [],
      onPickFolder: onPickFolder ?? (async () => {}),
      onRescanRoot: onRescanRoot ?? (async () => {}),
      onRequestRemoveRoot: onRemoveRoot ?? (() => {}),
      onRescanAll: onRescanAll ?? (async () => {}),
      onRenderAllThumbnails: onRenderAllThumbnails ?? (async () => {}),
      onRenderFailedThumbnails: onRenderFailedThumbnails ?? (async () => {}),
      onCancelThumbnailRender: onCancelThumbnailRender ?? (async () => {}),
      onCancelRemoveRootConfirm: onCancelRemoveRootConfirm ?? (() => {}),
      onConfirmRemoveRoot: onConfirmRemoveRoot ?? (async () => {})
    } satisfies ScanSettingsController);
  const importExportController =
    importExport ??
    ({
      isOperationLocked,
      operationState: importExportOperationState ?? { loading: false, message: "", progress: null },
      pendingImportDbSourcePath,
      onExportCsv: onExportCsv ?? (async () => {}),
      onImportCsv: onImportCsv ?? (async () => {}),
      onExportDbBundle: onExportDbBundle ?? (async () => {}),
      onImportDbBundle: onImportDbBundle ?? (async () => {}),
      onCancelImportDbOverwriteConfirm: onCancelImportDbOverwriteConfirm ?? (() => {}),
      onConfirmImportDbOverwriteConfirm: onConfirmImportDbOverwriteConfirm ?? (async () => {})
    } satisfies ImportExportSettingsController);
  const dangerZoneController =
    dangerZone ??
    ({
      isOperationLocked,
      operationState: dangerOperationState ?? { loading: false, message: "", progress: null },
      clearConfirmOpen,
      onOpenClearConfirm: onOpenClearConfirm ?? (() => {}),
      onCloseClearConfirm: onCloseClearConfirm ?? (() => {}),
      onConfirmClearYes: onConfirmClearYes ?? (async () => {})
    } satisfies DangerZoneSettingsController);
  const duplicatesController =
    duplicates ??
    ({
      isOperationLocked,
      operationState: duplicateOperationState ?? { loading: false, message: "", progress: null },
      duplicateDialogOpen,
      pendingDuplicateDeleteConfirm,
      groups: duplicateGroups,
      assetCount: duplicateAssetCount,
      thumbs: duplicateThumbs,
      renderingThumbnailIds: duplicateRenderingThumbnailIds,
      onQueueThumbnailsByIds: onQueueDuplicateThumbnailsByIds,
      onStart: onStartDuplicateScan ?? (async () => {}),
      onCloseDuplicateDialog: onCloseDuplicateDialog ?? (() => {}),
      onRescan: onRescanDuplicates ?? (async () => {}),
      onSaveAll: onSaveDuplicateChanges ?? (async () => {}),
      onCancelDuplicateDeleteConfirm: onCancelDuplicateDeleteConfirm ?? (() => {}),
      onConfirmDuplicateDeleteConfirm: onConfirmDuplicateDeleteConfirm ?? (async () => {})
    } satisfies DuplicatesSettingsController);

  const outerClasses = fullView
    ? "min-h-0 px-4 pb-4 sm:px-6"
    : "min-h-0 overflow-auto rounded-[var(--radius-panel)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-4 shadow-[var(--shadow-surface)] backdrop-blur-xl";
  const contentClasses = fullView
    ? "settings-card mx-auto grid w-full max-w-[1180px] gap-4 sm:gap-5"
    : "settings-card grid w-full max-w-[900px] gap-4";

  return (
    <>
      <section className={outerClasses}>
        <div className={contentClasses}>
          <AppearanceSection
            theme={appearanceController.theme}
            onThemeChange={appearanceController.onThemeChange}
            language={appearanceController.language}
            onLanguageChange={appearanceController.onLanguageChange}
          />

          <ScanSettingsSection
            highlighted={highlightScanSection}
            scanRoots={scanController.scanRoots}
            isOperationLocked={scanController.isOperationLocked}
            thumbnailBulkRunning={scanController.thumbnailBulkRunning}
            cancelThumbnailRunning={scanController.cancelThumbnailRunning}
            operationState={scanController.operationState}
            videoToolStatus={scanController.videoToolStatus}
            onPickFolder={scanController.onPickFolder}
            onRescanRoot={scanController.onRescanRoot}
            onRemoveRoot={scanController.onRequestRemoveRoot}
            onRescanAll={scanController.onRescanAll}
            onRenderAllThumbnails={scanController.onRenderAllThumbnails}
            onRenderFailedThumbnails={scanController.onRenderFailedThumbnails}
            onCancelThumbnailRender={scanController.onCancelThumbnailRender}
          />

          <div className="grid items-start gap-4 lg:grid-cols-2">
            <ImportExportSection
              isOperationLocked={importExportController.isOperationLocked}
              operationState={importExportController.operationState}
              onExportCsv={importExportController.onExportCsv}
              onImportCsv={importExportController.onImportCsv}
              onExportDbBundle={importExportController.onExportDbBundle}
              onImportDbBundle={importExportController.onImportDbBundle}
            />

            <DuplicatesSection
              isOperationLocked={duplicatesController.isOperationLocked}
              operationState={duplicatesController.operationState}
              duplicateGroups={duplicatesController.groups.length}
              duplicateAssets={duplicatesController.assetCount}
              onStart={duplicatesController.onStart}
            />

            <div className="lg:col-span-2">
              <DangerZoneSection
                isOperationLocked={dangerZoneController.isOperationLocked}
                operationState={dangerZoneController.operationState}
                onOpenClearConfirm={dangerZoneController.onOpenClearConfirm}
              />
            </div>
          </div>
        </div>
      </section>

      <RemoveScanRootConfirmDialog
        path={scanController.removeRootConfirmPath}
        isOperationLocked={scanController.isOperationLocked}
        onCancel={scanController.onCancelRemoveRootConfirm}
        onConfirm={scanController.onConfirmRemoveRoot}
      />

      <ClearLibraryConfirmDialog
        open={dangerZoneController.clearConfirmOpen}
        isOperationLocked={dangerZoneController.isOperationLocked}
        onClose={dangerZoneController.onCloseClearConfirm}
        onConfirm={dangerZoneController.onConfirmClearYes}
      />

      <DuplicateResolverDialog
        open={duplicatesController.duplicateDialogOpen}
        isOperationLocked={duplicatesController.isOperationLocked}
        operationState={duplicatesController.operationState}
        groups={duplicatesController.groups}
        thumbs={duplicatesController.thumbs}
        renderingThumbnailIds={duplicatesController.renderingThumbnailIds}
        onQueueThumbnailsByIds={duplicatesController.onQueueThumbnailsByIds}
        onClose={duplicatesController.onCloseDuplicateDialog}
        onRescan={duplicatesController.onRescan}
        onSaveAll={duplicatesController.onSaveAll}
      />

      <DuplicateDeleteConfirmDialog
        open={Boolean(duplicatesController.pendingDuplicateDeleteConfirm)}
        count={duplicatesController.pendingDuplicateDeleteConfirm?.changes.length ?? 0}
        deleteCount={duplicatesController.pendingDuplicateDeleteConfirm?.deleteCount ?? 0}
        isOperationLocked={duplicatesController.isOperationLocked}
        onCancel={duplicatesController.onCancelDuplicateDeleteConfirm}
        onConfirm={duplicatesController.onConfirmDuplicateDeleteConfirm}
      />

      <ImportDbOverwriteConfirmDialog
        open={Boolean(importExportController.pendingImportDbSourcePath)}
        isOperationLocked={importExportController.isOperationLocked}
        onCancel={importExportController.onCancelImportDbOverwriteConfirm}
        onConfirm={importExportController.onConfirmImportDbOverwriteConfirm}
      />
    </>
  );
}


