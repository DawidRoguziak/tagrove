import type { ScanProgress, ScanRoot, VideoToolStatus } from "../../types";
import type { DuplicateGroup } from "../../types";
import type { AppLanguage, AppTheme } from "../app/types";

export type SettingsOperationSection = "scan" | "importExport" | "danger" | "duplicates";

export interface SectionOperationState {
  loading: boolean;
  message: string;
  progress: ScanProgress | null;
}

export type DuplicateResolutionChange =
  | {
      assetId: number;
      type: "rename";
      nextFileName: string;
    }
  | {
      assetId: number;
      type: "delete";
    };

export interface PendingDuplicateDeleteConfirm {
  changes: DuplicateResolutionChange[];
  deleteCount: number;
}

export interface SettingsAppearanceController {
  theme: AppTheme;
  onThemeChange: (value: AppTheme) => void;
  language: AppLanguage;
  onLanguageChange: (value: AppLanguage) => void;
}

export interface ScanSettingsController {
  scanRoots: ScanRoot[];
  isOperationLocked: boolean;
  thumbnailBulkRunning: boolean;
  cancelThumbnailRunning: boolean;
  operationState: SectionOperationState;
  videoToolStatus?: VideoToolStatus | null;
  removeRootConfirmPath: string | null;
  refreshScanRoots: () => Promise<ScanRoot[]>;
  onPickFolder: () => Promise<void>;
  onSetAutoScan: (path: string, enabled: boolean) => Promise<void>;
  scanOnStartup: () => Promise<void>;
  onRescanRoot: (path: string) => Promise<void>;
  onRequestRemoveRoot: (path: string) => void;
  onRescanAll: () => Promise<void>;
  onRenderAllThumbnails: () => Promise<void>;
  onRenderFailedThumbnails: () => Promise<void>;
  onCancelThumbnailRender: () => Promise<void>;
  onCancelRemoveRootConfirm: () => void;
  onConfirmRemoveRoot: () => Promise<void>;
}

export interface ImportExportSettingsController {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  pendingImportDbSourcePath: string | null;
  onExportCsv: () => Promise<void>;
  onImportCsv: () => Promise<void>;
  onExportDbBundle: () => Promise<void>;
  onImportDbBundle: () => Promise<void>;
  onCancelImportDbOverwriteConfirm: () => void;
  onConfirmImportDbOverwriteConfirm: () => Promise<void>;
}

export interface DangerZoneSettingsController {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  clearConfirmOpen: boolean;
  onOpenClearConfirm: () => void;
  onCloseClearConfirm: () => void;
  onConfirmClearYes: () => Promise<void>;
}

export interface DuplicatesSettingsController {
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  duplicateDialogOpen: boolean;
  pendingDuplicateDeleteConfirm: PendingDuplicateDeleteConfirm | null;
  groups: DuplicateGroup[];
  assetCount: number;
  thumbs?: Record<number, string>;
  renderingThumbnailIds?: Record<number, true>;
  onQueueThumbnailsByIds?: (assetIds: number[]) => void;
  onStart: () => Promise<void>;
  onCloseDuplicateDialog: () => void;
  onRescan: () => Promise<void>;
  onSaveAll: (changes: DuplicateResolutionChange[]) => Promise<void>;
  onCancelDuplicateDeleteConfirm: () => void;
  onConfirmDuplicateDeleteConfirm: () => Promise<void>;
}
