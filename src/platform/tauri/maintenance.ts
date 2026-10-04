import { invoke } from "@tauri-apps/api/core";
import type {
  DuplicateScanSummary,
  DuplicateResolutionBatchChange,
  DuplicateResolutionBatchSummary,
  ClearLibrarySummary,
  CsvExportSummary,
  CsvImportSummary,
  DbBundleExportSummary,
  DbBundleImportSummary,
  DbRootMapping,
  DbBundleInspection,
  VideoToolStatus
} from "./contracts/maintenance";

export async function clearLibraryData(): Promise<ClearLibrarySummary> {
  return await invoke<ClearLibrarySummary>("clear_library_data");
}

export async function exportTagsCsv(path: string): Promise<CsvExportSummary> {
  return await invoke<CsvExportSummary>("export_tags_csv", { path });
}

export async function importTagsCsv(path: string): Promise<CsvImportSummary> {
  return await invoke<CsvImportSummary>("import_tags_csv", { path });
}

export async function exportDbBundle(path: string): Promise<DbBundleExportSummary> {
  return await invoke<DbBundleExportSummary>("export_db_bundle", { path });
}

export async function inspectDbBundle(path: string): Promise<DbBundleInspection> {
  return await invoke<DbBundleInspection>("inspect_db_bundle", { path });
}

export async function getVideoToolStatus(): Promise<VideoToolStatus> {
  return await invoke<VideoToolStatus>("get_video_tool_status");
}

export async function importDbBundle(
  path: string,
  rootMappings: DbRootMapping[] = []
): Promise<DbBundleImportSummary> {
  return await invoke<DbBundleImportSummary>("import_db_bundle", { path, rootMappings });
}

export async function findDuplicateAssets(): Promise<DuplicateScanSummary> {
  return await invoke<DuplicateScanSummary>("find_duplicate_assets");
}

export async function applyDuplicateResolutionBatch(
  scanRevision: number,
  changes: DuplicateResolutionBatchChange[]
): Promise<DuplicateResolutionBatchSummary> {
  return await invoke<DuplicateResolutionBatchSummary>("apply_duplicate_resolution_batch", {
    input: { scanRevision, changes }
  });
}
