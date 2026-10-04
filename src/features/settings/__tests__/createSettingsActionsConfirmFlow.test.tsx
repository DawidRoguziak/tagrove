import { renderHook } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
const apiMocks = vi.hoisted(() => {
  return {
    addScanRoot: vi.fn(),
    applyDuplicateResolutionBatch: vi.fn(),
    cancelRenderAllThumbnails: vi.fn(),
    clearLibraryData: vi.fn(),
    deleteAsset: vi.fn(),
    exportDbBundle: vi.fn(),
    exportTagsCsv: vi.fn(),
    findDuplicateAssets: vi.fn(),
    getVideoToolStatus: vi.fn(),
    inspectDbBundle: vi.fn(),
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
describe("useSettingsActions confirmation flows", () => {
  const setLoading = vi.fn();
  const refreshLibrary = vi.fn();
  const onRootRemoved = vi.fn();
  const onImportDbRestored = vi.fn();
  const onLibraryCleared = vi.fn();
  beforeEach(() => {
    setLoading.mockReset();
    refreshLibrary.mockReset();
    onRootRemoved.mockReset();
    onImportDbRestored.mockReset();
    onLibraryCleared.mockReset();
    refreshLibrary.mockResolvedValue(undefined);
    apiMocks.getVideoToolStatus.mockResolvedValue({
      ffmpeg_available: true,
      ffprobe_available: true
    });
    apiMocks.listScanRoots.mockReset();
    apiMocks.listScanRoots.mockResolvedValue([]);
    apiMocks.findDuplicateAssets.mockReset();
    apiMocks.findDuplicateAssets.mockResolvedValue({
      groups: [],
      duplicate_groups: 0,
      duplicate_assets: 0,
      revision: 2
    });
    apiMocks.applyDuplicateResolutionBatch.mockReset();
    apiMocks.applyDuplicateResolutionBatch.mockResolvedValue({
      status: "committed",
      revision: 2,
      results: [
        {
          asset_id: 1,
          status: "renamed",
          old_path: "C:/media/a.jpg",
          new_path: "C:/media/renamed.jpg",
          recovery_path: null
        },
        {
          asset_id: 2,
          status: "deleted",
          old_path: "C:/media/b.jpg",
          new_path: null,
          recovery_path: null
        }
      ],
      removed_thumbnails: 0
    });
    apiMocks.deleteAsset.mockReset();
    apiMocks.deleteAsset.mockResolvedValue({
      removed_assets: 1,
      removed_thumbnails: 0,
      removed_media_file: true
    });
    apiMocks.importDbBundle.mockReset();
    apiMocks.importDbBundle.mockResolvedValue({ restored_files: 0, restored_thumbnails: 0 });
    apiMocks.inspectDbBundle.mockReset();
    apiMocks.inspectDbBundle.mockResolvedValue({
      format_version: 1,
      source_platform: "linux",
      source_thumbs_dir: "/tmp/thumbs",
      roots: [],
      requires_mapping: false
    });
    apiMocks.getVideoToolStatus.mockReset();
    apiMocks.getVideoToolStatus.mockResolvedValue({
      ffmpeg_available: true,
      ffprobe_available: true
    });
    vi.mocked(open).mockResolvedValue(null);
  });
  it("stores duplicate delete confirmation and cancels it without applying changes", async () => {
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
      await result.duplicates.onSaveAll([{ assetId: 1, type: "delete" }]);
    });
    expect(result.duplicates.pendingDuplicateDeleteConfirm).toEqual({
      changes: [{ assetId: 1, type: "delete" }],
      deleteCount: 1
    });
    expect(apiMocks.applyDuplicateResolutionBatch).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    act(() => {
      result.duplicates.onCancelDuplicateDeleteConfirm();
    });
    expect(result.duplicates.pendingDuplicateDeleteConfirm).toBeNull();
    expect(apiMocks.applyDuplicateResolutionBatch).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    expect(result.duplicates.operationState.message).toBe("Applying duplicate changes cancelled.");
  });
  it("waits for confirmation before applying mixed duplicate changes", async () => {
    apiMocks.findDuplicateAssets.mockResolvedValueOnce({
      groups: [
        {
          file_name: "same.jpg",
          assets: [
            {
              id: 1,
              path: "C:/media/a.jpg",
              record_version: 3,
              size_bytes: 10
            },
            {
              id: 2,
              path: "C:/media/b.jpg",
              record_version: 4,
              size_bytes: 20
            }
          ]
        }
      ],
      duplicate_groups: 1,
      duplicate_assets: 2,
      revision: 7
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
    await act(async () => {
      await result.duplicates.onSaveAll([
        { assetId: 1, type: "rename", nextFileName: "renamed.jpg" },
        { assetId: 2, type: "delete" }
      ]);
    });
    expect(result.duplicates.pendingDuplicateDeleteConfirm).toEqual({
      changes: [
        { assetId: 1, type: "rename", nextFileName: "renamed.jpg" },
        { assetId: 2, type: "delete" }
      ],
      deleteCount: 1
    });
    expect(apiMocks.applyDuplicateResolutionBatch).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    await act(async () => {
      await result.duplicates.onConfirmDuplicateDeleteConfirm();
    });
    expect(apiMocks.applyDuplicateResolutionBatch).toHaveBeenCalledWith(7, [
      {
        type: "rename",
        assetId: 1,
        expectedPath: "C:/media/a.jpg",
        expectedRecordVersion: 3,
        newFileName: "renamed.jpg"
      },
      {
        type: "delete",
        assetId: 2,
        expectedPath: "C:/media/b.jpg",
        expectedRecordVersion: 4
      }
    ]);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.duplicates.pendingDuplicateDeleteConfirm).toBeNull();
    expect(result.duplicates.operationState.message).toBe("Applied 2 changes. Remaining groups: 0");
  });
  it("stores DB import confirmation and cancels it without importing", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/backup.zip");
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
      await result.importExport.onImportDbBundle();
    });
    expect(result.importExport.pendingImportDbSourcePath).toBe("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();
    act(() => {
      result.importExport.onCancelImportDbOverwriteConfirm();
    });
    expect(result.importExport.pendingImportDbSourcePath).toBeNull();
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();
    expect(onImportDbRestored).not.toHaveBeenCalled();
    expect(result.importExport.operationState.message).toBe("Database backup import cancelled.");
  });
  it("imports DB backup archive only after confirmation and inside the tag mutation barrier", async () => {
    const events: string[] = [];
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/backup.zip");
    apiMocks.importDbBundle.mockImplementationOnce(async () => {
      events.push("restore");
      return { restored_files: 2, restored_thumbnails: 5 };
    });
    onImportDbRestored.mockImplementationOnce(() => {
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
    await act(async () => {
      await result.importExport.onImportDbBundle();
    });
    expect(result.importExport.pendingImportDbSourcePath).toBe("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();
    await act(async () => {
      await result.importExport.onConfirmImportDbOverwriteConfirm();
    });
    expect(result.importExport.pendingImportDbSourcePath).toBeNull();
    expect(apiMocks.inspectDbBundle).toHaveBeenCalledWith("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).toHaveBeenCalledWith("C:/tmp/backup.zip", []);
    expect(onImportDbRestored).toHaveBeenCalledTimes(1);
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(events).toEqual(["barrier:start", "restore", "invalidate", "barrier:end"]);
    expect(apiMocks.listScanRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.importExport.operationState.message).toBe(
      "Database backup archive imported. Files: 2, thumbnails: 5"
    );
  });
  it("invalidates and refreshes library state when confirmed DB restore rejects", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/broken-backup.zip");
    apiMocks.importDbBundle.mockRejectedValueOnce(new Error("restore failed after swap"));
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
      await result.importExport.onImportDbBundle();
    });
    await act(async () => {
      await result.importExport.onConfirmImportDbOverwriteConfirm();
    });
    expect(apiMocks.importDbBundle).toHaveBeenCalledWith("C:/tmp/broken-backup.zip", []);
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(onImportDbRestored).toHaveBeenCalledTimes(1);
    expect(apiMocks.listScanRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(result.importExport.operationState.message).toContain("restore failed after swap");
  });
});
