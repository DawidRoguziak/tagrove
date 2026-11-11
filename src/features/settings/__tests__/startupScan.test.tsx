import { onSettled } from "solid-js";
import { LanguageProvider } from "../../../i18n/solid";
import { settle as act } from "../../../test/settle";
import { cleanup, renderHook, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createScanSettings } from "../createScanSettings";
import { createSettingsOperationRunner } from "../createSettingsOperationRunner";

const api = vi.hoisted(() => ({
  listScanRoots: vi.fn(),
  setScanRootAutoScan: vi.fn(),
  scanStartupRoots: vi.fn(),
  getVideoToolStatus: vi.fn(),
  addScanRoot: vi.fn(),
  scanFolder: vi.fn(),
  rescanAllRoots: vi.fn(),
  removeScanRoot: vi.fn(),
  cancelRenderAllThumbnails: vi.fn(),
  renderAllThumbnails: vi.fn(),
  renderFailedThumbnails: vi.fn()
}));
vi.mock("../../../platform/tauri/scanning", () => api);
vi.mock("../../../platform/tauri/maintenance", () => api);
vi.mock("../../../platform/tauri/thumbnails", () => api);
const events = vi.hoisted(() => ({ listen: vi.fn(), unlisten: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: events.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const refreshLibrary = vi.fn();
const setLoading = vi.fn();
const root = { path: "/media", auto_scan_on_startup: false };
const summary = { indexed: 2, removed: 0, failed: 0, completion: "complete" };

function useController(start = false) {
  const runner = createSettingsOperationRunner({ setLoading });
  const scan = createScanSettings({ runner, refreshLibrary, onRootRemoved: vi.fn() });
  onSettled(() => {
    if (start) void scan.scanOnStartup();
  });
  return scan;
}

describe("startup scanning", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    events.listen.mockResolvedValue(events.unlisten);
    api.listScanRoots.mockResolvedValue([root]);
    api.getVideoToolStatus.mockResolvedValue({ ffmpeg_available: true, ffprobe_available: true });
    api.scanStartupRoots.mockResolvedValue(null);
    refreshLibrary.mockResolvedValue(undefined);
  });
  afterEach(cleanup);

  it("saves the checkbox without scanning or refreshing the library", async () => {
    const { result } = renderHook(() => useController(), { wrapper: LanguageProvider });
    await act(async () => {
      await result.refreshScanRoots();
    });
    await act(async () => {
      await result.onSetAutoScan(root.path, true);
    });
    expect(api.setScanRootAutoScan).toHaveBeenCalledWith(root.path, true);
    expect(result.scanRoots).toEqual([{ ...root, auto_scan_on_startup: true }]);
    expect(api.scanStartupRoots).not.toHaveBeenCalled();
    expect(api.scanFolder).not.toHaveBeenCalled();
    expect(refreshLibrary).not.toHaveBeenCalled();
    expect(setLoading).not.toHaveBeenCalled();
  });

  it("keeps the saved checkbox value when persistence fails", async () => {
    api.setScanRootAutoScan.mockRejectedValue(new Error("write failed"));
    const { result } = renderHook(() => useController(), { wrapper: LanguageProvider });
    await act(async () => {
      await result.refreshScanRoots();
    });
    await act(async () => {
      await result.onSetAutoScan(root.path, true);
    });
    expect(result.scanRoots).toEqual([root]);
    expect(result.operationState.message).toContain("write failed");
    expect(result.isOperationLocked).toBe(false);
  });

  it("runs once per owner even when requested repeatedly, subscribes first, and refreshes after completion", async () => {
    api.scanStartupRoots.mockImplementation(async () => {
      expect(events.listen).toHaveBeenCalledWith("process-progress", expect.any(Function));
      return summary;
    });
    const { result } = renderHook(() => useController(true), { wrapper: LanguageProvider });
    await waitFor(() => expect(result.operationState.message).toContain("Indexed: 2"));
    await act(async () => {
      await result.scanOnStartup();
    });
    expect(api.scanStartupRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(events.unlisten).toHaveBeenCalledTimes(1);
    expect(result.isOperationLocked).toBe(false);
  });

  it("handles a launch with no enabled roots without a library refresh", async () => {
    const { result } = renderHook(() => useController(true), { wrapper: LanguageProvider });
    await waitFor(() => expect(events.unlisten).toHaveBeenCalled());
    expect(result.operationState.message).toBe("");
    expect(refreshLibrary).not.toHaveBeenCalled();
  });

  it("refreshes committed scan batches after failure and does not retry", async () => {
    api.scanStartupRoots.mockRejectedValue(new Error("scan failed"));
    const { result } = renderHook(() => useController(true), { wrapper: LanguageProvider });
    await waitFor(() => expect(result.operationState.message).toContain("scan failed"));
    await act(async () => {
      await result.scanOnStartup();
    });
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(api.listScanRoots).toHaveBeenCalledTimes(1);
    expect(api.scanStartupRoots).toHaveBeenCalledTimes(1);
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it("reports missing folders as a partial result", async () => {
    api.scanStartupRoots.mockResolvedValue({ ...summary, failed: 1, completion: "partial" });
    const { result } = renderHook(() => useController(true), { wrapper: LanguageProvider });
    await waitFor(() => expect(result.operationState.message).toMatch(/partial/i));
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
  });

  describe.each([
    ["rescan all", "rescanAllRoots", (c: ReturnType<typeof useController>) => c.onRescanAll()],
    [
      "rescan one root",
      "scanFolder",
      (c: ReturnType<typeof useController>) => c.onRescanRoot(root.path)
    ]
  ] as const)("manual %s", (_name, apiName, run) => {
    it("refreshes the library and roots after a rejection and keeps the error status", async () => {
      api[apiName].mockRejectedValue(new Error("scan exploded"));
      const { result } = renderHook(() => useController(), { wrapper: LanguageProvider });
      await act(async () => {
        await run(result);
      });
      expect(refreshLibrary).toHaveBeenCalledTimes(1);
      expect(api.listScanRoots).toHaveBeenCalledTimes(1);
      expect(result.operationState.message).toContain("scan exploded");
      expect(result.isOperationLocked).toBe(false);
    });

    it("still refreshes roots and keeps the error when the library refresh fails", async () => {
      api[apiName].mockRejectedValue(new Error("scan exploded"));
      refreshLibrary.mockRejectedValue(new Error("refresh failed"));
      const { result } = renderHook(() => useController(), { wrapper: LanguageProvider });
      await act(async () => {
        await run(result);
      });
      expect(api.listScanRoots).toHaveBeenCalledTimes(1);
      expect(result.operationState.message).toContain("scan exploded");
    });

    it("keeps the success path: one refresh each and the summary", async () => {
      api[apiName].mockResolvedValue(summary);
      const { result } = renderHook(() => useController(), { wrapper: LanguageProvider });
      await act(async () => {
        await run(result);
      });
      expect(refreshLibrary).toHaveBeenCalledTimes(1);
      expect(api.listScanRoots).toHaveBeenCalledTimes(1);
      expect(result.operationState.message).toContain("Indexed: 2");
    });
  });
});
