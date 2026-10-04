import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createSettingsOperationRunner } from "../createSettingsOperationRunner";
import { createSettings } from "../createSettings";
import { createAssetTagState } from "../../../library/metadata/createAssetTagState";
import { LanguageProvider } from "../../../i18n/solid";
import { settle } from "../../../test/settle";
import type { ScanProgress } from "../../../platform/tauri/contracts/scanning";
const api = vi.hoisted(() => ({
  listen: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
  getVideoToolStatus: vi.fn(),
  clearLibraryData: vi.fn(),
  importTagsCsv: vi.fn(),
  importDbBundle: vi.fn(),
  inspectDbBundle: vi.fn(),
  listScanRoots: vi.fn(),
  renderAllThumbnails: vi.fn(),
  cancelRenderAllThumbnails: vi.fn()
}));
vi.mock("../../../platform/tauri/events", () => api);
vi.mock("../../../platform/tauri/dialogs", () => api);
vi.mock("../../../platform/tauri/maintenance", () => api);
vi.mock("../../../platform/tauri/scanning", () => api);
vi.mock("../../../platform/tauri/thumbnails", () => api);
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
beforeEach(() => {
  vi.resetAllMocks();
  api.listen.mockResolvedValue(vi.fn());
  api.getVideoToolStatus.mockResolvedValue({ ffmpeg_available: true, ffprobe_available: true });
  api.listScanRoots.mockResolvedValue([]);
  api.inspectDbBundle.mockResolvedValue({ requires_mapping: false, roots: [] });
});
const options = {
  section: "scan",
  pendingMessage: "pending",
  errorPrefix: "failed",
  progressPhaseMatcher: (phase: string) => phase === "scanning"
} as const;
function runner() {
  const loading = vi.fn();
  return {
    ...renderHook(() => createSettingsOperationRunner({ setLoading: loading }), {
      wrapper: LanguageProvider
    }),
    loading
  };
}
describe("settings operation admission and listener ownership", () => {
  it("takes the lock synchronously and subscribes before dispatching", async () => {
    const registration = deferred<() => void>();
    api.listen.mockReturnValue(registration.promise);
    const view = runner();
    const first = vi.fn(async () => {}),
      second = vi.fn(async () => {}),
      unlisten = vi.fn();
    const running = view.result.runExclusiveOperation(options, first);
    expect(view.result.isOperationLocked).toBe(true);
    await view.result.runExclusiveOperation(options, second);
    expect(first).not.toHaveBeenCalled();
    expect(second).not.toHaveBeenCalled();
    registration.resolve(unlisten);
    await settle(() => running);
    expect(first).toHaveBeenCalledTimes(1);
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(view.loading.mock.calls).toEqual([[true], [false]]);
    expect(view.result.isOperationLocked).toBe(false);
  });
  it("releases the lock after listener rejection without running the action", async () => {
    api.listen.mockRejectedValueOnce(new Error("listener failed"));
    const view = runner(),
      action = vi.fn(async () => {});
    await settle(() => view.result.runExclusiveOperation(options, action));
    expect(action).not.toHaveBeenCalled();
    expect(view.result.scanOperationState.message).toContain("listener failed");
    expect(view.result.scanOperationState.loading).toBe(false);
    await settle(() => view.result.runExclusiveOperation(options, action));
    expect(action).toHaveBeenCalledTimes(1);
  });
  it("cleans up late registration after disposal without dispatch", async () => {
    const registration = deferred<() => void>();
    api.listen.mockReturnValue(registration.promise);
    const view = runner(),
      action = vi.fn(async () => {}),
      unlisten = vi.fn();
    const running = view.result.runExclusiveOperation(options, action);
    view.cleanup();
    registration.resolve(unlisten);
    await running;
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(action).not.toHaveBeenCalled();
    expect(view.result.isOperationLocked).toBe(false);
  });
  it("filters phases and ignores callbacks after completion or disposal", async () => {
    let progress!: (event: { payload: ScanProgress }) => void;
    const unlisten = vi.fn();
    api.listen.mockImplementation(async (_, handler) => {
      progress = handler;
      return unlisten;
    });
    const action = deferred<void>();
    const view = runner();
    const running = view.result.runExclusiveOperation(options, () => action.promise);
    await waitFor(() => expect(api.listen).toHaveBeenCalled());
    settle(() =>
      progress({ payload: { phase: "thumbs", message: "unrelated", processed: 9, total: 10 } })
    );
    expect(view.result.scanOperationState.message).toBe("pending");
    settle(() =>
      progress({ payload: { phase: "scanning", message: "raw", processed: 2, total: 10 } })
    );
    expect(view.result.scanOperationState.progress?.processed).toBe(2);
    const message = view.result.scanOperationState.message;
    action.resolve();
    await settle(() => running);
    settle(() =>
      progress({ payload: { phase: "scanning", message: "late", processed: 10, total: 10 } })
    );
    expect(view.result.scanOperationState.message).toBe(message);
    expect(view.result.scanOperationState.progress?.processed).toBe(2);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
  it("removes the active listener on owner disposal only once", async () => {
    const stop = vi.fn(),
      pending = deferred<void>();
    api.listen.mockResolvedValue(stop);
    const view = runner(),
      action = vi.fn(() => pending.promise);
    const running = view.result.runExclusiveOperation(options, action);
    await waitFor(() => expect(action).toHaveBeenCalled());
    view.cleanup();
    expect(stop).toHaveBeenCalledTimes(1);
    pending.resolve();
    await running;
    expect(stop).toHaveBeenCalledTimes(1);
  });
  it.each([
    "i18n:common.cancel",
    '{"key":"common.cancel"}',
    "common.cancel",
    "{malformed",
    "free form"
  ])("translates backend errors %s without changing other sections", async (raw) => {
    const view = runner();
    const before = view.result.importExportOperationState;
    await settle(() =>
      view.result.runExclusiveOperation(options, async () => {
        throw raw;
      })
    );
    expect(view.result.scanOperationState.message).toBe(
      `failed: ${raw.includes("common.cancel") ? "Cancel" : raw}`
    );
    expect(view.result.importExportOperationState).toEqual(before);
  });
});
function settings() {
  const refresh = vi.fn(async () => {}),
    invalidate = vi.fn();
  const view = renderHook(
    () => {
      const tags = createAssetTagState();
      const model = createSettings({
        setLoading: vi.fn(),
        refreshLibrary: refresh,
        onRootRemoved: vi.fn(),
        onTagCacheInvalidated: () => {
          invalidate();
          tags.reset();
        },
        onImportDbRestored: () => {
          invalidate();
          tags.reset();
        },
        onLibraryCleared: () => {
          invalidate();
          tags.reset();
        },
        runWithTagMutationBarrier: tags.runWithMutationBarrier
      });
      return { model, tags };
    },
    { wrapper: LanguageProvider }
  );
  return { ...view, ...view.result, refresh, invalidate };
}
describe("settings and actual metadata maintenance barrier", () => {
  it.each([
    "csv",
    "restore",
    "clear"
  ] as const)("drains writes, invalidates under the barrier and unlocks after uncertain %s failure", async (kind) => {
    const view = settings();
    const active = view.tags.beginMutation(1);
    if (!active) throw new Error("Missing mutation token");
    const backend = deferred<never>();
    const command =
      kind === "csv"
        ? api.importTagsCsv
        : kind === "restore"
          ? api.importDbBundle
          : api.clearLibraryData;
    command.mockReturnValue(backend.promise);
    let running: Promise<void>;
    if (kind === "csv") {
      api.open.mockResolvedValue("/synthetic/tags.csv");
      running = view.model.importExport.onImportCsv();
    } else if (kind === "restore") {
      api.open.mockResolvedValue("/synthetic/library.zip");
      await settle(() => view.model.importExport.onImportDbBundle());
      running = view.model.importExport.onConfirmImportDbOverwriteConfirm();
    } else {
      view.model.dangerZone.onOpenClearConfirm();
      running = view.model.dangerZone.onConfirmClearYes();
    }
    // Let dialog/listener registration reach maintenance admission.
    await settle(async () => {
      for (let i = 0; i < 8; i++) await Promise.resolve();
    });
    expect(command).not.toHaveBeenCalled();
    expect(view.tags.beginMutation(2)).toBeNull();
    settle(() => view.tags.settleMutation(active, ["saved"]));
    await waitFor(() => expect(command).toHaveBeenCalledTimes(1));
    view.invalidate.mockImplementation(() => {
      expect(view.tags.beginMutation(2)).toBeNull();
    });
    view.refresh.mockImplementation(async () => {
      const token = view.tags.beginMutation(3);
      expect(token).not.toBeNull();
      if (token) view.tags.settleMutation(token);
    });
    backend.reject(new Error("uncertain IPC failure"));
    await settle(() => running);
    expect(view.invalidate).toHaveBeenCalledTimes(1);
    expect(view.tags.get(1)).toBeNull();
    expect(view.refresh).toHaveBeenCalledTimes(1);
    expect(view.model.scan.isOperationLocked).toBe(false);
    expect(view.tags.beginMutation(4)).not.toBeNull();
  });
  it("keeps the original render lock while admitting only one stop request", async () => {
    const render = deferred<never>(),
      cancel = deferred<boolean>();
    api.renderAllThumbnails.mockReturnValue(render.promise);
    api.cancelRenderAllThumbnails.mockReturnValue(cancel.promise);
    const view = settings();
    const running = view.model.scan.onRenderAllThumbnails();
    await waitFor(() => expect(api.renderAllThumbnails).toHaveBeenCalled());
    const stop = view.model.scan.onCancelThumbnailRender();
    const repeated = view.model.scan.onCancelThumbnailRender();
    expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(1);
    cancel.resolve(true);
    await settle(() => Promise.all([stop, repeated]));
    expect(view.model.scan.isOperationLocked).toBe(true);
    expect(view.model.scan.thumbnailBulkRunning).toBe(true);
    expect(view.model.scan.cancelThumbnailRunning).toBe(false);
    render.reject(new Error("render failed"));
    await settle(() => running);
    expect(view.model.scan.isOperationLocked).toBe(false);
    expect(view.model.scan.thumbnailBulkRunning).toBe(false);
  });
  it("collects all root mappings before overwrite confirmation and consumes it only once", async () => {
    api.open
      .mockResolvedValueOnce("/synthetic/library.zip")
      .mockResolvedValueOnce("/synthetic/a")
      .mockResolvedValueOnce("/synthetic/b");
    api.inspectDbBundle.mockResolvedValue({ requires_mapping: true, roots: ["C:/a", "D:/b"] });
    api.importDbBundle.mockResolvedValue({ restored_files: 1, restored_thumbnails: 0 });
    const view = settings();
    await settle(() => view.model.importExport.onImportDbBundle());
    expect(api.importDbBundle).not.toHaveBeenCalled();
    expect(view.model.scan.isOperationLocked).toBe(false);
    await settle(() =>
      Promise.all([
        view.model.importExport.onConfirmImportDbOverwriteConfirm(),
        view.model.importExport.onConfirmImportDbOverwriteConfirm()
      ])
    );
    expect(api.importDbBundle).toHaveBeenCalledExactlyOnceWith("/synthetic/library.zip", [
      { sourceRoot: "C:/a", targetRoot: "/synthetic/a" },
      { sourceRoot: "D:/b", targetRoot: "/synthetic/b" }
    ]);
    expect(view.model.importExport.pendingImportDbSourcePath).toBeNull();
  });
  it("cancels root mapping without opening overwrite confirmation", async () => {
    api.open.mockResolvedValueOnce("/synthetic/library.zip").mockResolvedValueOnce(null);
    api.inspectDbBundle.mockResolvedValue({ requires_mapping: true, roots: ["C:/a"] });
    const view = settings();
    await settle(() => view.model.importExport.onImportDbBundle());
    expect(view.model.importExport.pendingImportDbSourcePath).toBeNull();
    expect(api.importDbBundle).not.toHaveBeenCalled();
    expect(view.model.scan.isOperationLocked).toBe(false);
  });
});
