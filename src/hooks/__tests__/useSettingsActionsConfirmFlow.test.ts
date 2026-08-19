import { act, renderHook } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsActions } from "../useSettingsActions";

const apiMocks = vi.hoisted(() => {
  return {
    addScanRoot: vi.fn(),
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
    renameAssetFile: vi.fn(),
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

vi.mock("../../api", async () => {
  const actual = await vi.importActual<typeof import("../../api")>("../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

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
    apiMocks.listScanRoots.mockReset();
    apiMocks.listScanRoots.mockResolvedValue([]);
    apiMocks.findDuplicateAssets.mockReset();
    apiMocks.findDuplicateAssets.mockResolvedValue({
      groups: [],
      duplicate_groups: 0,
      duplicate_assets: 0
    });
    apiMocks.renameAssetFile.mockReset();
    apiMocks.renameAssetFile.mockResolvedValue({
      asset_id: 1,
      old_path: "C:/media/a.jpg",
      new_path: "C:/media/b.jpg",
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
    const { result } = renderHook(() =>
      useSettingsActions({
        setLoading,
        refreshLibrary,
        onRootRemoved,
        onImportDbRestored,
        onLibraryCleared
      })
    );

    await act(async () => {
      await result.current.handleSaveDuplicateChanges([{ assetId: 1, type: "delete" }]);
    });

    expect(result.current.pendingDuplicateDeleteConfirm).toEqual({
      changes: [{ assetId: 1, type: "delete" }],
      deleteCount: 1
    });
    expect(apiMocks.renameAssetFile).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();

    act(() => {
      result.current.cancelPendingDuplicateDeleteConfirm();
    });

    expect(result.current.pendingDuplicateDeleteConfirm).toBeNull();
    expect(apiMocks.renameAssetFile).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    expect(result.current.duplicateOperationState.message).toBe("Applying duplicate changes cancelled.");
  });

  it("waits for confirmation before applying mixed duplicate changes", async () => {
    apiMocks.findDuplicateAssets.mockResolvedValueOnce({
      groups: [],
      duplicate_groups: 0,
      duplicate_assets: 0
    });

    const { result } = renderHook(() =>
      useSettingsActions({
        setLoading,
        refreshLibrary,
        onRootRemoved,
        onImportDbRestored,
        onLibraryCleared
      })
    );

    await act(async () => {
      await result.current.handleSaveDuplicateChanges([
        { assetId: 1, type: "rename", nextFileName: "renamed.jpg" },
        { assetId: 2, type: "delete" }
      ]);
    });

    expect(result.current.pendingDuplicateDeleteConfirm).toEqual({
      changes: [
        { assetId: 1, type: "rename", nextFileName: "renamed.jpg" },
        { assetId: 2, type: "delete" }
      ],
      deleteCount: 1
    });
    expect(apiMocks.renameAssetFile).not.toHaveBeenCalled();
    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.confirmPendingDuplicateDeleteConfirm();
    });

    expect(apiMocks.renameAssetFile).toHaveBeenCalledWith(1, "renamed.jpg");
    expect(apiMocks.deleteAsset).toHaveBeenCalledWith(2);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.current.pendingDuplicateDeleteConfirm).toBeNull();
    expect(result.current.duplicateOperationState.message).toBe("Applied 2 changes. Remaining groups: 0");
  });

  it("stores DB import confirmation and cancels it without importing", async () => {
    vi.mocked(open).mockResolvedValueOnce("C:/tmp/backup.zip");

    const { result } = renderHook(() =>
      useSettingsActions({
        setLoading,
        refreshLibrary,
        onRootRemoved,
        onImportDbRestored,
        onLibraryCleared
      })
    );

    await act(async () => {
      await result.current.handleImportDbBundle();
    });

    expect(result.current.pendingImportDbSourcePath).toBe("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();

    act(() => {
      result.current.cancelPendingImportDbOverwriteConfirm();
    });

    expect(result.current.pendingImportDbSourcePath).toBeNull();
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();
    expect(onImportDbRestored).not.toHaveBeenCalled();
    expect(result.current.importExportOperationState.message).toBe("Database backup import cancelled.");
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

    const { result } = renderHook(() =>
      useSettingsActions({
        setLoading,
        refreshLibrary,
        onRootRemoved,
        onImportDbRestored,
        onLibraryCleared,
        runWithTagMutationBarrier
      })
    );

    await act(async () => {
      await result.current.handleImportDbBundle();
    });

    expect(result.current.pendingImportDbSourcePath).toBe("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.confirmPendingImportDbOverwriteConfirm();
    });

    expect(result.current.pendingImportDbSourcePath).toBeNull();
    expect(apiMocks.inspectDbBundle).toHaveBeenCalledWith("C:/tmp/backup.zip");
    expect(apiMocks.importDbBundle).toHaveBeenCalledWith("C:/tmp/backup.zip", []);
    expect(onImportDbRestored).toHaveBeenCalledTimes(1);
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(events).toEqual(["barrier:start", "restore", "invalidate", "barrier:end"]);
    expect(apiMocks.listScanRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalled();
    expect(result.current.importExportOperationState.message).toBe(
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

    const { result } = renderHook(() =>
      useSettingsActions({
        setLoading,
        refreshLibrary,
        onRootRemoved,
        onImportDbRestored,
        onLibraryCleared,
        runWithTagMutationBarrier
      })
    );

    await act(async () => {
      await result.current.handleImportDbBundle();
    });
    await act(async () => {
      await result.current.confirmPendingImportDbOverwriteConfirm();
    });

    expect(apiMocks.importDbBundle).toHaveBeenCalledWith("C:/tmp/broken-backup.zip", []);
    expect(barrierCalls).toHaveBeenCalledTimes(1);
    expect(onImportDbRestored).toHaveBeenCalledTimes(1);
    expect(apiMocks.listScanRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(result.current.importExportOperationState.message).toContain("restore failed after swap");
  });
});
