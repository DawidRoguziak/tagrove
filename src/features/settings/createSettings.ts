import { createSettingsOperationRunner } from "./createSettingsOperationRunner";
import { createScanSettings } from "./createScanSettings";
import { createImportExportSettings } from "./createImportExportSettings";
import { createDuplicateSettings } from "./createDuplicateSettings";
import { createDangerZoneSettings } from "./createDangerZoneSettings";

interface Options {
  setLoading: (value: boolean) => void;
  refreshLibrary: () => Promise<void>;
  /** Called after a scan root and its assets were removed. */
  onRootRemoved?: () => void;
  onImportDbRestored: () => void;
  onLibraryCleared: () => void;
  onTagCacheInvalidated: () => void;
  runWithTagMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
}
export function createSettings(options: Options) {
  const runner = createSettingsOperationRunner(options);
  const scan = createScanSettings({ ...options, runner });
  const importExport = createImportExportSettings({
    ...options,
    runner,
    refreshScanRoots: scan.refreshScanRoots
  });
  const dangerZone = createDangerZoneSettings({
    ...options,
    runner,
    refreshScanRoots: scan.refreshScanRoots
  });
  const duplicates = createDuplicateSettings({ ...options, runner });
  return { scan, importExport, dangerZone, duplicates };
}
