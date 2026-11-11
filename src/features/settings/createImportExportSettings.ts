import type { ScanRoot } from "../../platform/tauri/contracts/scanning";
import { createSignal } from "solid-js";
import { open, save } from "../../platform/tauri/dialogs";
import { useLanguage } from "../../i18n/solid";
import {
  exportDbBundle,
  exportTagsCsv,
  importDbBundle,
  importTagsCsv,
  inspectDbBundle
} from "../../platform/tauri/maintenance";
import type { DbRootMapping } from "../../platform/tauri/contracts/maintenance";
import type { ImportExportSettingsController } from "./types";
import type { createSettingsOperationRunner } from "./createSettingsOperationRunner";
function formatExportTimestamp(date: Date): string {
  const year = String(date.getFullYear());
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}_${hours}-${minutes}`;
}
function buildExportFileName(prefix: string, extension: string): string {
  const timestamp = formatExportTimestamp(new Date());
  return `${prefix}-${timestamp}.${extension}`;
}
interface Options {
  runner: ReturnType<typeof createSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  refreshScanRoots: () => Promise<ScanRoot[]>;
  onImportDbRestored: () => void;
  onTagCacheInvalidated: () => void;
  runWithTagMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
}
export function createImportExportSettings({
  runner,
  refreshLibrary,
  refreshScanRoots,
  onImportDbRestored,
  onTagCacheInvalidated,
  runWithTagMutationBarrier
}: Options): ImportExportSettingsController {
  const { t } = useLanguage();
  const [pendingImport, publishPendingImport] = createSignal<{
    sourcePath: string;
    rootMappings: DbRootMapping[];
  } | null>(null);
  let pending: { sourcePath: string; rootMappings: DbRootMapping[] } | null = null;
  function setPending(value: typeof pending) {
    pending = value;
    publishPendingImport(value);
  }
  const onExportCsv = async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: () => t("settings.actions.pending.prepareCsvExport"),
        errorPrefix: () => t("settings.actions.errorPrefix.exportCsv")
      },
      async () => {
        const targetPath = await save({
          title: t("settings.actions.dialogs.exportTagsCsv"),
          defaultPath: buildExportFileName("tags-export", "csv"),
          filters: [{ name: "CSV", extensions: ["csv"] }]
        });
        if (typeof targetPath !== "string") {
          runner.setSectionMessage("importExport", () =>
            t("settings.actions.summary.csvExportCancelled")
          );
          return;
        }
        runner.setSectionMessage("importExport", () => t("settings.actions.summary.csvExporting"));
        const summary = await exportTagsCsv(targetPath);
        runner.setSectionMessage("importExport", () =>
          t("settings.actions.summary.csvExported", { rows: summary.rows })
        );
      }
    );
  };
  const onImportCsv = async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: () => t("settings.actions.pending.prepareCsvImport"),
        errorPrefix: () => t("settings.actions.errorPrefix.importCsv")
      },
      async () => {
        const sourcePath = await open({
          title: t("settings.actions.dialogs.importTagsCsv"),
          multiple: false,
          directory: false,
          filters: [{ name: "CSV", extensions: ["csv"] }]
        });
        if (typeof sourcePath !== "string") {
          runner.setSectionMessage("importExport", () =>
            t("settings.actions.summary.csvImportCancelled")
          );
          return;
        }
        runner.setSectionMessage("importExport", () => t("settings.actions.summary.csvImporting"));
        let summary: Awaited<ReturnType<typeof importTagsCsv>>;
        try {
          summary = await runWithTagMutationBarrier(async () => {
            try {
              return await importTagsCsv(sourcePath);
            } finally {
              onTagCacheInvalidated();
            }
          });
        } catch (error) {
          await refreshLibrary().catch(() => {});
          throw error;
        }
        await refreshLibrary();
        runner.setSectionMessage("importExport", () =>
          t("settings.actions.summary.csvImported", {
            rowsRead: summary.rows_read,
            rowsApplied: summary.rows_applied,
            assetsMatched: summary.assets_matched,
            assetsUpdated: summary.assets_updated
          })
        );
      }
    );
  };
  const onExportDbBundle = async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: () => t("settings.actions.pending.prepareDbExport"),
        errorPrefix: () => t("settings.actions.errorPrefix.exportDb")
      },
      async () => {
        const targetPath = await save({
          title: t("settings.actions.dialogs.exportDbArchive"),
          defaultPath: buildExportFileName("media-backup", "zip"),
          filters: [{ name: "ZIP", extensions: ["zip"] }]
        });
        if (typeof targetPath !== "string") {
          runner.setSectionMessage("importExport", () =>
            t("settings.actions.summary.dbExportCancelled")
          );
          return;
        }
        runner.setSectionMessage("importExport", () => t("settings.actions.summary.dbExporting"));
        const summary = await exportDbBundle(targetPath);
        runner.setSectionMessage("importExport", () =>
          t("settings.actions.summary.dbExported", {
            files: summary.copied_files,
            thumbnails: summary.copied_thumbnails
          })
        );
      }
    );
  };
  const onImportDbBundle = async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: () => t("settings.actions.pending.prepareDbImport"),
        errorPrefix: () => t("settings.actions.errorPrefix.importDb")
      },
      async () => {
        const sourcePath = await open({
          title: t("settings.actions.dialogs.selectDbArchive"),
          multiple: false,
          directory: false,
          filters: [{ name: "ZIP", extensions: ["zip"] }]
        });
        if (typeof sourcePath !== "string") {
          runner.setSectionMessage("importExport", () =>
            t("settings.actions.summary.dbImportCancelled")
          );
          return;
        }
        const inspection = await inspectDbBundle(sourcePath);
        const rootMappings: DbRootMapping[] = [];
        if (inspection.requires_mapping) {
          for (const sourceRoot of inspection.roots) {
            const targetRoot = await open({
              title: `${t("settings.scan.chooseFolder")}: ${sourceRoot}`,
              multiple: false,
              directory: true
            });
            if (typeof targetRoot !== "string") {
              runner.setSectionMessage("importExport", () =>
                t("settings.actions.summary.dbImportCancelled")
              );
              return;
            }
            rootMappings.push({ sourceRoot, targetRoot });
          }
        }
        setPending({ sourcePath, rootMappings });
      }
    );
  };
  const onCancelImportDbOverwriteConfirm = () => {
    if (runner.isOperationLocked) {
      return;
    }
    setPending(null);
    runner.setSectionMessage("importExport", () => t("settings.actions.summary.dbImportCancelled"));
  };
  const runImportDbBundle = async (sourcePath: string, rootMappings: DbRootMapping[]) => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: () => t("settings.actions.pending.prepareDbImport"),
        errorPrefix: () => t("settings.actions.errorPrefix.importDb")
      },
      async () => {
        runner.setSectionMessage("importExport", () => t("settings.actions.summary.dbImporting"));
        let summary: Awaited<ReturnType<typeof importDbBundle>>;
        try {
          summary = await runWithTagMutationBarrier(async () => {
            try {
              return await importDbBundle(sourcePath, rootMappings);
            } finally {
              // Restore can reject after swapping live files. Invalidate identity-bound state
              // before maintenance ownership is released, regardless of the command result.
              onImportDbRestored();
            }
          });
        } catch (error) {
          await Promise.allSettled([refreshScanRoots(), refreshLibrary()]);
          throw error;
        }
        await refreshScanRoots();
        await refreshLibrary();
        runner.setSectionMessage("importExport", () =>
          t("settings.actions.summary.dbImported", {
            files: summary.restored_files,
            thumbnails: summary.restored_thumbnails
          })
        );
      }
    );
  };
  const onConfirmImportDbOverwriteConfirm = async () => {
    if (runner.isOperationLocked) return;
    const confirmation = pending;
    if (!confirmation) {
      return;
    }
    setPending(null);
    await runImportDbBundle(confirmation.sourcePath, confirmation.rootMappings);
  };
  return {
    get isOperationLocked() {
      return runner.isOperationLocked;
    },
    get operationState() {
      return runner.importExportOperationState;
    },
    get pendingImportDbSourcePath() {
      return pendingImport()?.sourcePath ?? null;
    },
    onExportCsv,
    onImportCsv,
    onExportDbBundle,
    onImportDbBundle,
    onCancelImportDbOverwriteConfirm,
    onConfirmImportDbOverwriteConfirm
  };
}
