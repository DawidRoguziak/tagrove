import type { ScanRoot } from "../../../types";
import { useCallback, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  exportDbBundle,
  exportTagsCsv,
  importDbBundle,
  importTagsCsv,
  inspectDbBundle
} from "../../../api";
import type { DbRootMapping } from "../../../types";
import type { ImportExportSettingsController } from "../types";
import type { useSettingsOperationRunner } from "./useSettingsOperationRunner";

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

interface UseImportExportSettingsActionsOptions {
  runner: ReturnType<typeof useSettingsOperationRunner>;
  refreshLibrary: () => Promise<void>;
  refreshScanRoots: () => Promise<ScanRoot[]>;
  onImportDbRestored: () => void;
  onTagCacheInvalidated: () => void;
  runWithTagMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
}

export function useImportExportSettingsActions({
  runner,
  refreshLibrary,
  refreshScanRoots,
  onImportDbRestored,
  onTagCacheInvalidated,
  runWithTagMutationBarrier
}: UseImportExportSettingsActionsOptions): ImportExportSettingsController {
  const { t } = useTranslation();
  const [pendingImportDbSourcePath, setPendingImportDbSourcePath] = useState<string | null>(null);
  const [pendingRootMappings, setPendingRootMappings] = useState<DbRootMapping[]>([]);

  const onExportCsv = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: t("settings.actions.pending.prepareCsvExport"),
        errorPrefix: t("settings.actions.errorPrefix.exportCsv")
      },
      async () => {
        const targetPath = await save({
          title: t("settings.actions.dialogs.exportTagsCsv"),
          defaultPath: buildExportFileName("tags-export", "csv"),
          filters: [{ name: "CSV", extensions: ["csv"] }]
        });

        if (typeof targetPath !== "string") {
          runner.setSectionMessage("importExport", t("settings.actions.summary.csvExportCancelled"));
          return;
        }

        runner.setSectionMessage("importExport", t("settings.actions.summary.csvExporting"));
        const summary = await exportTagsCsv(targetPath);
        runner.setSectionMessage(
          "importExport",
          t("settings.actions.summary.csvExported", { rows: summary.rows })
        );
      }
    );
  }, [runner, t]);

  const onImportCsv = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: t("settings.actions.pending.prepareCsvImport"),
        errorPrefix: t("settings.actions.errorPrefix.importCsv")
      },
      async () => {
        const sourcePath = await open({
          title: t("settings.actions.dialogs.importTagsCsv"),
          multiple: false,
          directory: false,
          filters: [{ name: "CSV", extensions: ["csv"] }]
        });

        if (typeof sourcePath !== "string") {
          runner.setSectionMessage("importExport", t("settings.actions.summary.csvImportCancelled"));
          return;
        }

        runner.setSectionMessage("importExport", t("settings.actions.summary.csvImporting"));
        let summary!: Awaited<ReturnType<typeof importTagsCsv>>;
        let importFailed = false;
        let importError: unknown;
        await runWithTagMutationBarrier(async () => {
          try {
            summary = await importTagsCsv(sourcePath);
          } catch (error) {
            importFailed = true;
            importError = error;
          } finally {
            // Keep maintenance ownership through invalidation on both commit and rollback.
            // An IPC failure is not a commit signal even though the DB transaction is atomic.
            onTagCacheInvalidated();
          }
        });

        if (importFailed) {
          await refreshLibrary().catch(() => {});
          throw importError;
        }
        await refreshLibrary();
        runner.setSectionMessage(
          "importExport",
          t("settings.actions.summary.csvImported", {
            rowsRead: summary.rows_read,
            rowsApplied: summary.rows_applied,
            assetsMatched: summary.assets_matched,
            assetsUpdated: summary.assets_updated
          })
        );
      }
    );
  }, [onTagCacheInvalidated, refreshLibrary, runWithTagMutationBarrier, runner, t]);

  const onExportDbBundle = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: t("settings.actions.pending.prepareDbExport"),
        errorPrefix: t("settings.actions.errorPrefix.exportDb")
      },
      async () => {
        const targetPath = await save({
          title: t("settings.actions.dialogs.exportDbArchive"),
          defaultPath: buildExportFileName("media-backup", "zip"),
          filters: [{ name: "ZIP", extensions: ["zip"] }]
        });

        if (typeof targetPath !== "string") {
          runner.setSectionMessage("importExport", t("settings.actions.summary.dbExportCancelled"));
          return;
        }

        runner.setSectionMessage("importExport", t("settings.actions.summary.dbExporting"));
        const summary = await exportDbBundle(targetPath);
        runner.setSectionMessage(
          "importExport",
          t("settings.actions.summary.dbExported", {
            files: summary.copied_files,
            thumbnails: summary.copied_thumbnails
          })
        );
      }
    );
  }, [runner, t]);

  const onImportDbBundle = useCallback(async () => {
    await runner.runExclusiveOperation(
      {
        section: "importExport",
        pendingMessage: t("settings.actions.pending.prepareDbImport"),
        errorPrefix: t("settings.actions.errorPrefix.importDb")
      },
      async () => {
        const sourcePath = await open({
          title: t("settings.actions.dialogs.selectDbArchive"),
          multiple: false,
          directory: false,
          filters: [{ name: "ZIP", extensions: ["zip"] }]
        });

        if (typeof sourcePath !== "string") {
          runner.setSectionMessage("importExport", t("settings.actions.summary.dbImportCancelled"));
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
              runner.setSectionMessage("importExport", t("settings.actions.summary.dbImportCancelled"));
              return;
            }
            rootMappings.push({ sourceRoot, targetRoot });
          }
        }
        setPendingRootMappings(rootMappings);
        setPendingImportDbSourcePath(sourcePath);
      }
    );
  }, [runner, t]);

  const onCancelImportDbOverwriteConfirm = useCallback(() => {
    if (runner.isOperationLocked) {
      return;
    }

    setPendingImportDbSourcePath(null);
    setPendingRootMappings([]);
    runner.setSectionMessage("importExport", t("settings.actions.summary.dbImportCancelled"));
  }, [runner, t]);

  const runImportDbBundle = useCallback(
    async (sourcePath: string, rootMappings: DbRootMapping[]) => {
      await runner.runExclusiveOperation(
        {
          section: "importExport",
          pendingMessage: t("settings.actions.pending.prepareDbImport"),
          errorPrefix: t("settings.actions.errorPrefix.importDb")
        },
        async () => {
          runner.setSectionMessage("importExport", t("settings.actions.summary.dbImporting"));
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
          runner.setSectionMessage(
            "importExport",
            t("settings.actions.summary.dbImported", {
              files: summary.restored_files,
              thumbnails: summary.restored_thumbnails
            })
          );
        }
      );
    },
    [
      onImportDbRestored,
      refreshLibrary,
      refreshScanRoots,
      runWithTagMutationBarrier,
      runner,
      t
    ]
  );

  const onConfirmImportDbOverwriteConfirm = useCallback(async () => {
    const sourcePath = pendingImportDbSourcePath;
    if (!sourcePath) {
      return;
    }

    setPendingImportDbSourcePath(null);
    const rootMappings = pendingRootMappings;
    setPendingRootMappings([]);
    await runImportDbBundle(sourcePath, rootMappings);
  }, [pendingImportDbSourcePath, pendingRootMappings, runImportDbBundle]);

  return useMemo(
    () => ({
      isOperationLocked: runner.isOperationLocked,
      operationState: runner.importExportOperationState,
      pendingImportDbSourcePath,
      onExportCsv,
      onImportCsv,
      onExportDbBundle,
      onImportDbBundle,
      onCancelImportDbOverwriteConfirm,
      onConfirmImportDbOverwriteConfirm
    }),
    [
      onCancelImportDbOverwriteConfirm,
      onConfirmImportDbOverwriteConfirm,
      onExportCsv,
      onExportDbBundle,
      onImportCsv,
      onImportDbBundle,
      pendingImportDbSourcePath,
      runner.importExportOperationState,
      runner.isOperationLocked
    ]
  );
}
