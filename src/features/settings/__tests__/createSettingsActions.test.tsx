import { renderHook, waitFor } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ScanProgress } from "../../../types";
import { createSettings } from "../createSettings";
import { LanguageProvider } from "../../../i18n/solid";
import { settle as act } from "../../../test/settle";
function settings(
  options: Omit<
    Parameters<typeof createSettings>[0],
    "onTagCacheInvalidated" | "runWithTagMutationBarrier"
  > &
    Partial<
      Pick<
        Parameters<typeof createSettings>[0],
        "onTagCacheInvalidated" | "runWithTagMutationBarrier"
      >
    >
) {
  return createSettings({
    onTagCacheInvalidated: () => {},
    runWithTagMutationBarrier: (operation) => operation(),
    ...options
  });
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}
const apiMocks = vi.hoisted(() => {
  return {
    getVideoToolStatus: vi.fn(),
    addScanRoot: vi.fn(),
    applyDuplicateResolutionBatch: vi.fn(),
    cancelRenderAllThumbnails: vi.fn(),
    clearLibraryData: vi.fn(),
    deleteAsset: vi.fn(),
    exportDbBundle: vi.fn(),
    exportTagsCsv: vi.fn(),
    findDuplicateAssets: vi.fn(),
    importDbBundle: vi.fn(),
    importTagsCsv: vi.fn(),
    listScanRoots: vi.fn(),
    removeScanRoot: vi.fn(),
    renderAllThumbnails: vi.fn(),
    renderFailedThumbnails: vi.fn(),
    scanFolder: vi.fn(),
    rescanAllRoots: vi.fn()
  };
});
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn()
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => {
    return () => {};
  })
}));
vi.mock("../../../platform/tauri/scanning", async () => ({
  ...(await vi.importActual<typeof import("../../../platform/tauri/scanning")>(
    "../../../platform/tauri/scanning"
  )),
  ...apiMocks
}));
vi.mock("../../../platform/tauri/thumbnails", async () => ({
  ...(await vi.importActual<typeof import("../../../platform/tauri/thumbnails")>(
    "../../../platform/tauri/thumbnails"
  )),
  ...apiMocks
}));
vi.mock("../../../platform/tauri/maintenance", async () => ({
  ...(await vi.importActual<typeof import("../../../platform/tauri/maintenance")>(
    "../../../platform/tauri/maintenance"
  )),
  ...apiMocks
}));
describe("useSettingsActions", () => {
  let progressHandler: ((event: { payload: ScanProgress }) => void) | null;
  const setLoading = vi.fn();
  const refreshLibrary = vi.fn();
  const onRootRemoved = vi.fn();
  const onImportDbRestored = vi.fn();
  const onLibraryCleared = vi.fn();
  const onTagCacheInvalidated = vi.fn();
  beforeEach(() => {
    progressHandler = null;
    setLoading.mockReset();
    refreshLibrary.mockReset();
    onRootRemoved.mockReset();
    onImportDbRestored.mockReset();
    onLibraryCleared.mockReset();
    onTagCacheInvalidated.mockReset();
    Object.values(apiMocks).forEach((mock) => mock.mockReset());
    vi.mocked(open).mockReset();
    vi.mocked(save).mockReset();
    vi.mocked(listen).mockReset();
    refreshLibrary.mockResolvedValue(undefined);
    apiMocks.getVideoToolStatus.mockResolvedValue({
      ffmpeg_available: true,
      ffprobe_available: true
    });
    apiMocks.listScanRoots.mockResolvedValue([]);
    apiMocks.addScanRoot.mockResolvedValue(undefined);
    apiMocks.removeScanRoot.mockResolvedValue({ removed_assets: 0, removed_thumbnails: 0 });
    apiMocks.scanFolder.mockResolvedValue({ indexed: 0, removed: 0, failed: 0 });
    apiMocks.rescanAllRoots.mockResolvedValue({ indexed: 0, removed: 0, failed: 0 });
    apiMocks.renderAllThumbnails.mockResolvedValue({
      generated: 0,
      stale: 0,
      failed: 0,
      skipped_failed: 0,
      processed: 0,
      total: 0,
      cancelled: false
    });
    apiMocks.renderFailedThumbnails.mockResolvedValue({
      generated: 0,
      stale: 0,
      failed: 0,
      skipped_failed: 0,
      processed: 0,
      total: 0,
      cancelled: false
    });
    apiMocks.cancelRenderAllThumbnails.mockResolvedValue(true);
    apiMocks.clearLibraryData.mockResolvedValue({
      removed_assets: 0,
      removed_roots: 0,
      removed_thumbnails: 0
    });
    apiMocks.exportTagsCsv.mockResolvedValue({ rows: 0 });
    apiMocks.importTagsCsv.mockResolvedValue({
      rows_read: 0,
      rows_applied: 0,
      assets_matched: 0,
      assets_updated: 0
    });
    apiMocks.exportDbBundle.mockResolvedValue({ copied_files: 0, copied_thumbnails: 0 });
    apiMocks.importDbBundle.mockResolvedValue({ restored_files: 0, restored_thumbnails: 0 });
    apiMocks.findDuplicateAssets.mockResolvedValue({
      groups: [],
      duplicate_groups: 0,
      duplicate_assets: 0,
      revision: 1
    });
    apiMocks.applyDuplicateResolutionBatch.mockResolvedValue({
      status: "committed",
      revision: 2,
      results: [],
      removed_thumbnails: 0
    });
    apiMocks.deleteAsset.mockResolvedValue({
      removed_assets: 1,
      removed_thumbnails: 0,
      removed_media_file: true
    });
    vi.mocked(open).mockResolvedValue(null);
    vi.mocked(save).mockResolvedValue(null);
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      progressHandler = handler as (event: { payload: ScanProgress }) => void;
      return () => {};
    });
  });
  it("refreshes scan roots and adds a root from picker selection", async () => {
    apiMocks.listScanRoots
      .mockResolvedValueOnce([{ path: "C:/library", auto_scan_on_startup: false }])
      .mockResolvedValueOnce([{ path: "C:/library", auto_scan_on_startup: false }])
      .mockResolvedValueOnce([
        { path: "C:/library", auto_scan_on_startup: false },
        { path: "C:/media", auto_scan_on_startup: false }
      ]);
    vi.mocked(open).mockResolvedValueOnce("C:/media");
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.scan.refreshScanRoots();
    });
    expect(result.scan.scanRoots).toEqual([{ path: "C:/library", auto_scan_on_startup: false }]);
    await act(async () => {
      await result.scan.onPickFolder();
    });
    expect(apiMocks.addScanRoot).toHaveBeenCalledWith("C:/media");
    expect(result.scan.operationState.message).toBe("Folder added to scan list: C:/media");
  });
  it("updates scan section from progress events during rescan", async () => {
    const pending = deferred<{
      indexed: number;
      removed: number;
      failed: number;
    }>();
    apiMocks.rescanAllRoots.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    act(() => {
      void result.scan.onRescanAll();
    });
    await waitFor(() => {
      expect(result.scan.isOperationLocked).toBe(true);
      expect(result.scan.operationState.loading).toBe(true);
      expect(vi.mocked(listen)).toHaveBeenCalled();
    });
    act(() => {
      progressHandler?.({
        payload: {
          phase: "scanning",
          processed: 2,
          total: 5,
          message: "Scanning 2/5"
        }
      });
    });
    expect(result.scan.operationState.message).toBe("Scanning 2/5");
    expect(result.scan.operationState.progress?.processed).toBe(2);
    pending.resolve({ indexed: 5, removed: 1, failed: 0 });
    await waitFor(() => {
      expect(result.scan.isOperationLocked).toBe(false);
      expect(result.scan.operationState.loading).toBe(false);
      expect(result.scan.operationState.message).toBe("Done. Indexed: 5, removed: 1, failed: 0");
    });
  });
  it("rescans single root and refreshes library", async () => {
    apiMocks.scanFolder.mockResolvedValueOnce({ indexed: 3, removed: 1, failed: 0 });
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.scan.onRescanRoot("C:/media");
    });
    expect(apiMocks.scanFolder).toHaveBeenCalledWith("C:/media");
    expect(refreshLibrary).toHaveBeenCalled();
    expect(apiMocks.listScanRoots).toHaveBeenCalled();
    expect(result.scan.operationState.message).toBe("Done. Indexed: 3, removed: 1, failed: 0");
  });
  it("blocks other operations while exclusive operation is running", async () => {
    const pending = deferred<{
      generated: number;
      stale: number;
      failed: number;
      skipped_failed: number;
      processed: number;
      total: number;
      cancelled: boolean;
    }>();
    apiMocks.renderAllThumbnails.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    act(() => {
      void result.scan.onRenderAllThumbnails();
    });
    await waitFor(() => {
      expect(result.scan.thumbnailBulkRunning).toBe(true);
      expect(result.scan.isOperationLocked).toBe(true);
    });
    await act(async () => {
      await result.importExport.onImportCsv();
    });
    expect(apiMocks.importTagsCsv).not.toHaveBeenCalled();
    pending.resolve({
      generated: 3,
      stale: 0,
      failed: 1,
      skipped_failed: 2,
      processed: 6,
      total: 10,
      cancelled: true
    });
    await waitFor(() => {
      expect(result.scan.thumbnailBulkRunning).toBe(false);
      expect(result.scan.isOperationLocked).toBe(false);
      expect(result.scan.operationState.message).toContain("Cancelled at 6/10");
    });
  });
  it("closes confirmation and clears library data inside the tag mutation barrier", async () => {
    const events: string[] = [];
    apiMocks.clearLibraryData.mockImplementationOnce(async () => {
      events.push("clear");
      return {
        removed_assets: 7,
        removed_roots: 2,
        removed_thumbnails: 11
      };
    });
    onLibraryCleared.mockImplementationOnce(() => {
      events.push("invalidate");
    });
    const barrierCalls = vi.fn();
    const runWithTagMutationBarrier = async <T,>(operation: () => Promise<T>): Promise<T> => {
      barrierCalls();
      events.push("barrier:start");
      try {
        return await operation();
      } finally {
        events.push("barrier:end");
      }
    };
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared,
          runWithTagMutationBarrier
        }),
      { wrapper: LanguageProvider }
    );
    act(() => {
      result.dangerZone.onOpenClearConfirm();
    });
    await act(async () => {
      await result.dangerZone.onConfirmClearYes();
    });
    expect(result.dangerZone.clearConfirmOpen).toBe(false);
    expect(onLibraryCleared).toHaveBeenCalledTimes(1);
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(events).toEqual(["barrier:start", "clear", "invalidate", "barrier:end"]);
    expect(result.dangerZone.operationState.message).toBe(
      "Library cleared. Assets: 7, roots: 2, thumbnails: 11"
    );
  });
  it("invalidates and refreshes library state when clear library rejects", async () => {
    apiMocks.clearLibraryData.mockRejectedValueOnce(new Error("partial clear failure"));
    const barrierCalls = vi.fn();
    const runWithTagMutationBarrier = async <T,>(operation: () => Promise<T>): Promise<T> => {
      barrierCalls();
      return operation();
    };
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared,
          runWithTagMutationBarrier
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.dangerZone.onConfirmClearYes();
    });
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(onLibraryCleared).toHaveBeenCalledTimes(1);
    expect(apiMocks.listScanRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(result.dangerZone.operationState.message).toContain("partial clear failure");
  });
  it("handles cancelled export/import dialog selections", async () => {
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.importExport.onExportCsv();
    });
    expect(apiMocks.exportTagsCsv).not.toHaveBeenCalled();
    expect(result.importExport.operationState.message).toBe("CSV export cancelled.");
    await act(async () => {
      await result.importExport.onImportCsv();
    });
    expect(apiMocks.importTagsCsv).not.toHaveBeenCalled();
    expect(result.importExport.operationState.message).toBe("CSV import cancelled.");
  });
  it("opens duplicate dialog and scans duplicate groups", async () => {
    apiMocks.findDuplicateAssets.mockResolvedValueOnce({
      groups: [
        {
          file_name: "same.jpg",
          assets: [
            {
              id: 1,
              path: "C:/media/a/same.jpg",
              record_version: 1,
              size_bytes: 10
            },
            {
              id: 2,
              path: "C:/media/b/same.jpg",
              record_version: 1,
              size_bytes: 10
            }
          ]
        }
      ],
      duplicate_groups: 1,
      duplicate_assets: 2,
      revision: 1
    });
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.duplicates.onStart();
    });
    expect(result.duplicates.duplicateDialogOpen).toBe(true);
    expect(result.duplicates.groups).toHaveLength(1);
    expect(result.duplicates.assetCount).toBe(2);
    expect(result.duplicates.operationState.message).toBe(
      "Duplicate scan complete. Groups: 1, assets: 2"
    );
  });
  it("handles folder picker add and cancellation", async () => {
    apiMocks.listScanRoots
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ path: "C:/media", auto_scan_on_startup: false }]);
    vi.mocked(open).mockResolvedValueOnce("C:/media").mockResolvedValueOnce(null);
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.scan.onPickFolder();
    });
    expect(apiMocks.addScanRoot).toHaveBeenCalledWith("C:/media");
    expect(result.scan.operationState.message).toBe("Folder added to scan list: C:/media");
    expect(setLoading).not.toHaveBeenCalled();
    await act(async () => {
      await result.scan.onPickFolder();
    });
    expect(apiMocks.addScanRoot).toHaveBeenLastCalledWith("C:/media");
    expect(result.scan.operationState.message).toBe("Folder selection cancelled.");
  });
  it("waits for remove confirmation before deleting root", async () => {
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      result.scan.onRequestRemoveRoot("C:/media");
    });
    expect(result.scan.removeRootConfirmPath).toBe("C:/media");
    expect(apiMocks.removeScanRoot).not.toHaveBeenCalled();
    expect(onRootRemoved).not.toHaveBeenCalled();
    await act(async () => {
      result.scan.onCancelRemoveRootConfirm();
    });
    expect(result.scan.removeRootConfirmPath).toBeNull();
    expect(apiMocks.removeScanRoot).not.toHaveBeenCalled();
    apiMocks.removeScanRoot.mockResolvedValueOnce({ removed_assets: 4, removed_thumbnails: 9 });
    await act(async () => {
      result.scan.onRequestRemoveRoot("C:/media");
    });
    await act(async () => {
      await result.scan.onConfirmRemoveRoot();
    });
    expect(result.scan.removeRootConfirmPath).toBeNull();
    expect(apiMocks.removeScanRoot).toHaveBeenCalledWith("C:/media");
    expect(onRootRemoved).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.scan.operationState.message).toBe("Removed root. Assets: 4, thumbnails: 9");
  });
  it("reports when stop-thumbnail request is rejected or fails", async () => {
    apiMocks.cancelRenderAllThumbnails.mockResolvedValueOnce(false);
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.scan.onCancelThumbnailRender();
    });
    expect(result.scan.operationState.message).toBe("No thumbnail bulk render is running.");
    apiMocks.cancelRenderAllThumbnails.mockRejectedValueOnce(new Error("network"));
    await act(async () => {
      await result.scan.onCancelThumbnailRender();
    });
    expect(result.scan.operationState.message).toContain("Cancel thumbnail render error");
    expect(result.scan.cancelThumbnailRunning).toBe(false);
  });
  it("reports when duplicate-change save receives no changes", async () => {
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.duplicates.onSaveAll([]);
    });
    expect(result.duplicates.operationState.message).toBe("No pending duplicate changes to apply");
    expect(apiMocks.applyDuplicateResolutionBatch).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
  });
  it("rejects duplicate rename changes with a blank file name", async () => {
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.duplicates.onSaveAll([{ assetId: 1, type: "rename", nextFileName: "   " }]);
    });
    expect(apiMocks.applyDuplicateResolutionBatch).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    expect(result.duplicates.operationState.message).toContain("File name cannot be empty");
  });
  it("imports CSV and refreshes library state on success", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/tags.csv");
    apiMocks.importTagsCsv.mockResolvedValueOnce({
      rows_read: 12,
      rows_applied: 8,
      assets_matched: 6,
      assets_updated: 4
    });
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared,
          onTagCacheInvalidated
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.importExport.onImportCsv();
    });
    expect(apiMocks.importTagsCsv).toHaveBeenCalledWith("C:/tmp/tags.csv");
    expect(onTagCacheInvalidated).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.importExport.operationState.message).toBe(
      "CSV imported. Rows read: 12, applied: 8, assets matched by file name: 6, assets updated: 4. One row can match files with the same name in multiple folders."
    );
  });
  it("invalidates tag caches and refreshes after a rejected CSV import", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/partial-tags.csv");
    apiMocks.importTagsCsv.mockRejectedValueOnce(new Error("malformed later row"));
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared,
          onTagCacheInvalidated
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.importExport.onImportCsv();
    });
    expect(apiMocks.importTagsCsv).toHaveBeenCalledWith("C:/tmp/partial-tags.csv");
    expect(onTagCacheInvalidated).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(result.importExport.operationState.message).toContain("malformed later row");
  });
  it("exports database bundle and reports copied files summary", async () => {
    vi.mocked(save).mockResolvedValueOnce("C:/tmp/backup.zip");
    apiMocks.exportDbBundle.mockResolvedValueOnce({
      copied_files: 3,
      copied_thumbnails: 9
    });
    const { result } = renderHook(
      () =>
        settings({
          setLoading,
          refreshLibrary,
          onRootRemoved,
          onImportDbRestored,
          onLibraryCleared
        }),
      { wrapper: LanguageProvider }
    );
    await act(async () => {
      await result.importExport.onExportDbBundle();
    });
    expect(apiMocks.exportDbBundle).toHaveBeenCalledWith("C:/tmp/backup.zip");
    expect(result.importExport.operationState.message).toBe(
      "Database backup archive exported. Files: 3, thumbnails: 9"
    );
  });
});
