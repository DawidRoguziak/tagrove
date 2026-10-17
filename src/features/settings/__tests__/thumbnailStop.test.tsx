import { fireEvent, render, renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LanguageProvider } from "../../../i18n/solid";
import type { ScanProgress, ScanSummary } from "../../../platform/tauri/contracts/scanning";
import type { ThumbnailRenderSummary } from "../../../platform/tauri/contracts/thumbnails";
import { settle } from "../../../test/settle";
import { createScanSettings } from "../createScanSettings";
import { createSettingsOperationRunner } from "../createSettingsOperationRunner";
import { ScanSettingsSection } from "../sections/ScanSettingsSection";

const api = vi.hoisted(() => ({
  listen: vi.fn(),
  getVideoToolStatus: vi.fn(),
  listScanRoots: vi.fn(),
  rescanAllRoots: vi.fn(),
  renderAllThumbnails: vi.fn(),
  renderFailedThumbnails: vi.fn(),
  cancelRenderAllThumbnails: vi.fn()
}));
vi.mock("../../../platform/tauri/events", () => api);
vi.mock("../../../platform/tauri/maintenance", () => api);
vi.mock("../../../platform/tauri/scanning", () => api);
vi.mock("../../../platform/tauri/thumbnails", () => api);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const summary: ThumbnailRenderSummary = {
  stale: 0,
  generated: 2,
  failed: 0,
  skipped_failed: 0,
  processed: 2,
  total: 2,
  cancelled: false
};
const scanSummary: ScanSummary = { completion: "complete", indexed: 3, removed: 0, failed: 0 };
let progress: (event: { payload: ScanProgress }) => void;
beforeEach(() => {
  vi.resetAllMocks();
  api.listen.mockImplementation(async (_, handler) => {
    progress = handler;
    return vi.fn();
  });
  api.getVideoToolStatus.mockResolvedValue({ ffmpeg_available: true, ffprobe_available: true });
  api.listScanRoots.mockResolvedValue([{ path: "/synthetic/media", auto_scan_on_startup: false }]);
});
function setup() {
  const refreshLibrary = vi.fn(async () => {});
  const view = renderHook(
    () =>
      createScanSettings({
        runner: createSettingsOperationRunner({ setLoading: vi.fn() }),
        refreshLibrary,
        onRootRemoved: vi.fn()
      }),
    { wrapper: LanguageProvider }
  );
  return { ...view, refreshLibrary };
}
const replies = ["accepted", "idle", "rejected"] as const;
function reply(stop: ReturnType<typeof deferred<boolean>>, outcome: (typeof replies)[number]) {
  if (outcome === "rejected") stop.reject(new Error("stop failed"));
  else stop.resolve(outcome === "accepted");
}

describe.each(["all", "failed"] as const)("%s thumbnail render Stop ownership", (mode) => {
  function start(model: ReturnType<typeof createScanSettings>) {
    return mode === "all" ? model.onRenderAllThumbnails() : model.onRenderFailedThumbnails();
  }
  function command() {
    return mode === "all" ? api.renderAllThumbnails : api.renderFailedThumbnails;
  }

  describe.each(["success", "failure"] as const)("after render %s", (terminal) => {
    describe.each([false, true])("subsequent scan: %s", (newerScan) => {
      it.each(replies)("ignores late %s Stop", async (outcome) => {
        const renderReply = deferred<ThumbnailRenderSummary>();
        const stopReply = deferred<boolean>();
        const scanReply = deferred<ScanSummary>();
        command().mockReturnValue(renderReply.promise);
        api.cancelRenderAllThumbnails.mockReturnValue(stopReply.promise);
        api.rescanAllRoots.mockReturnValue(scanReply.promise);
        const view = setup();
        const rendering = start(view.result);
        await waitFor(() => expect(view.result.thumbnailBulkRunning).toBe(true));
        const stopping = view.result.onCancelThumbnailRender();
        if (terminal === "success") renderReply.resolve(summary);
        else renderReply.reject(new Error("render failed"));
        await settle(() => rendering);
        expect(view.result.isOperationLocked).toBe(false);
        if (terminal === "failure")
          expect(view.result.operationState.message).toContain("render failed");
        else expect(view.refreshLibrary).toHaveBeenCalledTimes(1);
        let scanning: Promise<void> | undefined;
        if (newerScan) {
          api.rescanAllRoots.mockClear();
          scanning = view.result.onRescanAll();
          await waitFor(() => expect(api.rescanAllRoots).toHaveBeenCalledTimes(1));
          settle(() =>
            progress({ payload: { phase: "scanning", processed: 2, total: 7, message: "scan" } })
          );
          expect(view.result.operationState.progress?.processed).toBe(2);
        }
        const authoritative = view.result.operationState;
        reply(stopReply, outcome);
        await settle(() => stopping);
        expect(view.result.operationState).toEqual(authoritative);
        expect(view.result.cancelThumbnailRunning).toBe(false);
        if (scanning) {
          scanReply.resolve(scanSummary);
          await settle(() => scanning);
          expect(view.result.isOperationLocked).toBe(false);
        }
        view.cleanup();
      });
    });
  });

  it.each(
    replies
  )("publishes active-render %s Stop and allows another request", async (outcome) => {
    const renderReply = deferred<ThumbnailRenderSummary>();
    const stopReply = deferred<boolean>();
    command().mockReturnValue(renderReply.promise);
    api.cancelRenderAllThumbnails.mockReturnValue(stopReply.promise);
    const view = setup();
    const rendering = start(view.result);
    await waitFor(() => expect(view.result.thumbnailBulkRunning).toBe(true));
    const stopping = view.result.onCancelThumbnailRender();
    await view.result.onCancelThumbnailRender();
    expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(1);
    reply(stopReply, outcome);
    await settle(() => stopping);
    expect(view.result.operationState.message).toContain(
      outcome === "rejected"
        ? "stop failed"
        : outcome === "accepted"
          ? "Stop requested"
          : "No thumbnail bulk render"
    );
    expect(view.result.isOperationLocked).toBe(true);
    expect(view.result.cancelThumbnailRunning).toBe(false);
    api.cancelRenderAllThumbnails.mockResolvedValueOnce(true);
    await settle(() => view.result.onCancelThumbnailRender());
    expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(2);
    renderReply.resolve(summary);
    await settle(() => rendering);
    expect(view.result.thumbnailBulkRunning).toBe(false);
    expect(view.result.isOperationLocked).toBe(false);
  });
});

it.each(
  replies
)("a retired %s Stop cannot clear a newer render's pending Stop", async (outcome) => {
  const first = deferred<ThumbnailRenderSummary>(),
    second = deferred<ThumbnailRenderSummary>();
  const oldStop = deferred<boolean>(),
    newStop = deferred<boolean>();
  api.renderAllThumbnails.mockReturnValue(first.promise);
  api.renderFailedThumbnails.mockReturnValue(second.promise);
  api.cancelRenderAllThumbnails
    .mockReturnValueOnce(oldStop.promise)
    .mockReturnValueOnce(newStop.promise);
  const view = setup();
  const rendering = view.result.onRenderAllThumbnails();
  await waitFor(() => expect(view.result.thumbnailBulkRunning).toBe(true));
  const stopping = view.result.onCancelThumbnailRender();
  first.resolve(summary);
  await settle(() => rendering);
  expect(view.result.cancelThumbnailRunning).toBe(false);
  const retrying = view.result.onRenderFailedThumbnails();
  await waitFor(() => expect(api.renderFailedThumbnails).toHaveBeenCalledTimes(1));
  const nextStop = view.result.onCancelThumbnailRender();
  settle(() => {});
  expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(2);
  expect(view.result.cancelThumbnailRunning).toBe(true);
  const state = view.result.operationState;
  reply(oldStop, outcome);
  await settle(() => stopping);
  expect(view.result.operationState).toEqual(state);
  expect(view.result.cancelThumbnailRunning).toBe(true);
  await view.result.onCancelThumbnailRender();
  expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(2);
  newStop.resolve(true);
  await settle(() => nextStop);
  expect(view.result.cancelThumbnailRunning).toBe(false);
  second.resolve(summary);
  await settle(() => retrying);
});

it("rejects Stop without an active render, including before listener registration and after disposal", async () => {
  const registration = deferred<() => void>();
  api.listen.mockReturnValue(registration.promise);
  const view = setup();
  await view.result.onCancelThumbnailRender();
  const rendering = view.result.onRenderAllThumbnails();
  await view.result.onCancelThumbnailRender();
  expect(api.cancelRenderAllThumbnails).not.toHaveBeenCalled();
  view.cleanup();
  registration.resolve(vi.fn());
  await rendering;
  await view.result.onCancelThumbnailRender();
  expect(api.cancelRenderAllThumbnails).not.toHaveBeenCalled();
  expect(api.renderAllThumbnails).not.toHaveBeenCalled();
});

it.each(replies)("ignores pending %s Stop after owner disposal", async (outcome) => {
  const renderReply = deferred<ThumbnailRenderSummary>(),
    stopReply = deferred<boolean>();
  api.renderAllThumbnails.mockReturnValue(renderReply.promise);
  api.cancelRenderAllThumbnails.mockReturnValue(stopReply.promise);
  const view = setup();
  const rendering = view.result.onRenderAllThumbnails();
  await waitFor(() => expect(view.result.thumbnailBulkRunning).toBe(true));
  const stopping = view.result.onCancelThumbnailRender();
  const state = view.result.operationState;
  view.cleanup();
  reply(stopReply, outcome);
  await settle(() => stopping);
  expect(view.result.operationState).toEqual(state);
  await view.result.onCancelThumbnailRender();
  expect(api.cancelRenderAllThumbnails).toHaveBeenCalledTimes(1);
  renderReply.reject(new Error("disposed render"));
  await rendering;
});

it("keeps rendered scan progress after Stop and render failure, with an attached root", async () => {
  const renderReply = deferred<ThumbnailRenderSummary>(),
    stopReply = deferred<boolean>(),
    scanReply = deferred<ScanSummary>();
  api.renderAllThumbnails.mockReturnValue(renderReply.promise);
  api.cancelRenderAllThumbnails.mockReturnValue(stopReply.promise);
  api.rescanAllRoots.mockReturnValue(scanReply.promise);
  const view = setup();
  await settle(() => view.result.refreshScanRoots());
  const model = view.result;
  let stopping: Promise<void> | undefined;
  const ui = render(() => (
    <LanguageProvider>
      <ScanSettingsSection
        scanRoots={model.scanRoots}
        onSetAutoScan={model.onSetAutoScan}
        isOperationLocked={model.isOperationLocked}
        thumbnailBulkRunning={model.thumbnailBulkRunning}
        cancelThumbnailRunning={model.cancelThumbnailRunning}
        operationState={model.operationState}
        onPickFolder={model.onPickFolder}
        onRescanRoot={model.onRescanRoot}
        onRemoveRoot={model.onRequestRemoveRoot}
        onRescanAll={model.onRescanAll}
        onRenderAllThumbnails={model.onRenderAllThumbnails}
        onRenderFailedThumbnails={model.onRenderFailedThumbnails}
        onCancelThumbnailRender={() => {
          stopping = model.onCancelThumbnailRender();
        }}
      />
    </LanguageProvider>
  ));
  fireEvent.click(ui.getByRole("button", { name: "Render thumbnails for all indexed assets" }));
  const stop = await ui.findByRole("button", { name: "Stop thumbnail render" });
  fireEvent.click(stop);
  await waitFor(() => expect(stop).toBeDisabled());
  renderReply.reject(new Error("render failed"));
  const rescan = ui.getByRole("button", { name: "Rescan all" });
  await waitFor(() => expect(rescan).toBeEnabled());
  expect(ui.getByText(/render failed/)).toBeInTheDocument();
  fireEvent.click(rescan);
  await waitFor(() => expect(api.rescanAllRoots).toHaveBeenCalledTimes(1));
  settle(() =>
    progress({ payload: { phase: "scanning", processed: 2, total: 7, message: "scan" } })
  );
  const message = ui.getByRole("status").textContent;
  if (!stopping) throw new Error("Stop was not dispatched");
  stopReply.resolve(true);
  await settle(() => stopping);
  expect(ui.getByRole("status").textContent).toBe(message);
  expect(ui.getByRole("progressbar")).toHaveAttribute("value", "2");
  scanReply.resolve(scanSummary);
  await waitFor(() => expect(rescan).toBeEnabled());
  expect(ui.queryByRole("button", { name: "Stop thumbnail render" })).not.toBeInTheDocument();
});
