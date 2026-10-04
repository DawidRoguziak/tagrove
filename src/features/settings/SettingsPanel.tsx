import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";
import { AboutSection } from "./sections/AboutSection";
import { SettingsNavigation } from "./SettingsNavigation";
import { ClearLibraryConfirmDialog } from "./ClearLibraryConfirmDialog";
import { DuplicateDeleteConfirmDialog } from "./DuplicateDeleteConfirmDialog";
import { DuplicateResolverDialog } from "./DuplicateResolverDialog";
import { ImportDbOverwriteConfirmDialog } from "./ImportDbOverwriteConfirmDialog";
import { RemoveScanRootConfirmDialog } from "./RemoveScanRootConfirmDialog";
import { LanguageSection } from "./sections/LanguageSection";
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
} from "../../components/settings/types";

interface Props {
  highlightScanSection: boolean;
  appearance: SettingsAppearanceController;
  scan: ScanSettingsController;
  importExport: ImportExportSettingsController;
  dangerZone: DangerZoneSettingsController;
  duplicates: DuplicatesSettingsController;
  thumbnailStore: ThumbnailStore;
  onQueueThumbnailsByIds: (ids: number[]) => void;
}
export function SettingsPanel(props: Props) {
  return (
    <>
      <section class="min-h-0 px-4 pb-4 sm:px-6">
        <div class="settings-card settings-layout">
          <SettingsNavigation />
          <div class="grid min-w-0 content-start gap-5">
            <div id="settings-scan" class="settings-section" tabindex={-1}>
              <ScanSettingsSection
                highlighted={props.highlightScanSection}
                scanRoots={props.scan.scanRoots}
                onSetAutoScan={props.scan.onSetAutoScan}
                isOperationLocked={props.scan.isOperationLocked}
                thumbnailBulkRunning={props.scan.thumbnailBulkRunning}
                cancelThumbnailRunning={props.scan.cancelThumbnailRunning}
                operationState={props.scan.operationState}
                videoToolStatus={props.scan.videoToolStatus}
                onPickFolder={props.scan.onPickFolder}
                onRescanRoot={props.scan.onRescanRoot}
                onRemoveRoot={props.scan.onRequestRemoveRoot}
                onRescanAll={props.scan.onRescanAll}
                onRenderAllThumbnails={props.scan.onRenderAllThumbnails}
                onRenderFailedThumbnails={props.scan.onRenderFailedThumbnails}
                onCancelThumbnailRender={props.scan.onCancelThumbnailRender}
              />
            </div>
            <div id="settings-appearance" class="settings-section" tabindex={-1}>
              <AppearanceSection
                theme={props.appearance.theme}
                onThemeChange={props.appearance.onThemeChange}
              />
            </div>
            <div id="settings-language" class="settings-section" tabindex={-1}>
              <LanguageSection
                language={props.appearance.language}
                onLanguageChange={props.appearance.onLanguageChange}
              />
            </div>
            <div id="settings-import-export" class="settings-section" tabindex={-1}>
              <ImportExportSection
                isOperationLocked={props.importExport.isOperationLocked}
                operationState={props.importExport.operationState}
                onExportCsv={props.importExport.onExportCsv}
                onImportCsv={props.importExport.onImportCsv}
                onExportDbBundle={props.importExport.onExportDbBundle}
                onImportDbBundle={props.importExport.onImportDbBundle}
              />
            </div>
            <div id="settings-duplicates" class="settings-section" tabindex={-1}>
              <DuplicatesSection
                isOperationLocked={props.duplicates.isOperationLocked}
                operationState={props.duplicates.operationState}
                duplicateGroups={props.duplicates.groups.length}
                duplicateAssets={props.duplicates.assetCount}
                onStart={props.duplicates.onStart}
              />
            </div>
            <div id="settings-danger" class="settings-section" tabindex={-1}>
              <DangerZoneSection
                isOperationLocked={props.dangerZone.isOperationLocked}
                operationState={props.dangerZone.operationState}
                onOpenClearConfirm={props.dangerZone.onOpenClearConfirm}
              />
            </div>
            <div id="settings-about" class="settings-section" tabindex={-1}>
              <AboutSection />
            </div>
          </div>
        </div>
      </section>

      <RemoveScanRootConfirmDialog
        path={props.scan.removeRootConfirmPath}
        isOperationLocked={props.scan.isOperationLocked}
        onCancel={props.scan.onCancelRemoveRootConfirm}
        onConfirm={props.scan.onConfirmRemoveRoot}
      />

      <ClearLibraryConfirmDialog
        open={props.dangerZone.clearConfirmOpen}
        isOperationLocked={props.dangerZone.isOperationLocked}
        onClose={props.dangerZone.onCloseClearConfirm}
        onConfirm={props.dangerZone.onConfirmClearYes}
      />

      <DuplicateResolverDialog
        open={props.duplicates.duplicateDialogOpen}
        isOperationLocked={props.duplicates.isOperationLocked}
        operationState={props.duplicates.operationState}
        groups={props.duplicates.groups}
        thumbnailStore={props.thumbnailStore}
        onQueueThumbnailsByIds={props.onQueueThumbnailsByIds}
        onClose={props.duplicates.onCloseDuplicateDialog}
        onRescan={props.duplicates.onRescan}
        onSaveAll={props.duplicates.onSaveAll}
      />

      <DuplicateDeleteConfirmDialog
        open={Boolean(props.duplicates.pendingDuplicateDeleteConfirm)}
        count={props.duplicates.pendingDuplicateDeleteConfirm?.changes.length ?? 0}
        deleteCount={props.duplicates.pendingDuplicateDeleteConfirm?.deleteCount ?? 0}
        isOperationLocked={props.duplicates.isOperationLocked}
        onCancel={props.duplicates.onCancelDuplicateDeleteConfirm}
        onConfirm={props.duplicates.onConfirmDuplicateDeleteConfirm}
      />

      <ImportDbOverwriteConfirmDialog
        open={Boolean(props.importExport.pendingImportDbSourcePath)}
        isOperationLocked={props.importExport.isOperationLocked}
        onCancel={props.importExport.onCancelImportDbOverwriteConfirm}
        onConfirm={props.importExport.onConfirmImportDbOverwriteConfirm}
      />
    </>
  );
}
