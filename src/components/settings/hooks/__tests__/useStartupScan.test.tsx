import { StrictMode, useEffect } from "react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useScanSettingsActions } from "../useScanSettingsActions";
import { useSettingsOperationRunner } from "../useSettingsOperationRunner";

const api = vi.hoisted(() => ({
  listScanRoots: vi.fn(), setScanRootAutoScan: vi.fn(), scanStartupRoots: vi.fn(),
  getVideoToolStatus: vi.fn(), addScanRoot: vi.fn(), scanFolder: vi.fn(),
  rescanAllRoots: vi.fn(), removeScanRoot: vi.fn(), cancelRenderAllThumbnails: vi.fn(),
  renderAllThumbnails: vi.fn(), renderFailedThumbnails: vi.fn()
}));
vi.mock("../../../../api", () => api);
const events = vi.hoisted(() => ({ listen: vi.fn(), unlisten: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: events.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const refreshLibrary = vi.fn();
const setLoading = vi.fn();
const root = { path: "/media", auto_scan_on_startup: false };
const summary = { indexed: 2, removed: 0, failed: 0, completion: "complete" };

function useController(start = false) {
  const runner = useSettingsOperationRunner({ setLoading });
  const scan = useScanSettingsActions({ runner, refreshLibrary, onRootRemoved: vi.fn() });
  useEffect(() => {
    if (start) void scan.scanOnStartup();
    // Exercise mount initialization, including StrictMode replay.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
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
    const { result } = renderHook(() => useController());
    await act(async () => { await result.current.refreshScanRoots(); });
    await act(async () => { await result.current.onSetAutoScan(root.path, true); });
    expect(api.setScanRootAutoScan).toHaveBeenCalledWith(root.path, true);
    expect(result.current.scanRoots).toEqual([{ ...root, auto_scan_on_startup: true }]);
    expect(api.scanStartupRoots).not.toHaveBeenCalled();
    expect(api.scanFolder).not.toHaveBeenCalled();
    expect(refreshLibrary).not.toHaveBeenCalled();
    expect(setLoading).not.toHaveBeenCalled();
  });

  it("keeps the saved checkbox value when persistence fails", async () => {
    api.setScanRootAutoScan.mockRejectedValue(new Error("write failed"));
    const { result } = renderHook(() => useController());
    await act(async () => { await result.current.refreshScanRoots(); });
    await act(async () => { await result.current.onSetAutoScan(root.path, true); });
    expect(result.current.scanRoots).toEqual([root]);
    expect(result.current.operationState.message).toContain("write failed");
    expect(result.current.isOperationLocked).toBe(false);
  });

  it("runs once in StrictMode, subscribes first, and refreshes after completion", async () => {
    api.scanStartupRoots.mockImplementation(async () => {
      expect(events.listen).toHaveBeenCalledWith("process-progress", expect.any(Function));
      return summary;
    });
    const { result, rerender } = renderHook(() => useController(true), { wrapper: StrictMode });
    await waitFor(() => expect(result.current.operationState.message).toContain("Indexed: 2"));
    rerender();
    await act(async () => { await result.current.scanOnStartup(); });
    expect(api.scanStartupRoots).toHaveBeenCalledTimes(1);
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(events.unlisten).toHaveBeenCalledTimes(1);
    expect(result.current.isOperationLocked).toBe(false);
  });

  it("handles an empty or already consumed launch without a library refresh", async () => {
    const { result } = renderHook(() => useController(true));
    await waitFor(() => expect(events.unlisten).toHaveBeenCalled());
    expect(result.current.operationState.message).toBe("");
    expect(refreshLibrary).not.toHaveBeenCalled();
  });

  it("refreshes committed scan batches after failure and does not retry", async () => {
    api.scanStartupRoots.mockRejectedValue(new Error("scan failed"));
    const { result } = renderHook(() => useController(true));
    await waitFor(() => expect(result.current.operationState.message).toContain("scan failed"));
    await act(async () => { await result.current.scanOnStartup(); });
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
    expect(api.listScanRoots).toHaveBeenCalledTimes(1);
    expect(api.scanStartupRoots).toHaveBeenCalledTimes(1);
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it("reports missing folders as a partial result", async () => {
    api.scanStartupRoots.mockResolvedValue({ ...summary, failed: 1, completion: "partial" });
    const { result } = renderHook(() => useController(true));
    await waitFor(() => expect(result.current.operationState.message).toMatch(/partial/i));
    expect(refreshLibrary).toHaveBeenCalledTimes(1);
  });
});
