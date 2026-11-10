import { cleanup, renderHook, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { settle as act } from "../../../../test/settle";
import { afterEach, beforeEach, describe, expect, it, type Mock, onTestFinished, vi } from "vitest";
import { createNativeVideoSession, type SessionOptions } from "../createNativeVideoSession";
import type {
  MpvVideoEvent,
  NativeVideoControlLabels
} from "../../../../platform/tauri/contracts/video";

const api = vi.hoisted(() => ({
  beginVideoOpen: vi.fn(),
  cancelVideoOpen: vi.fn(),
  openVideo: vi.fn(),
  closeVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  setVideoControlLabels: vi.fn()
}));
vi.mock("../../../../platform/tauri/video", () => api);
const labels: NativeVideoControlLabels = {
  play: "Play",
  pause: "Pause",
  mute: "Mute",
  unmute: "Unmute",
  volume: "Volume",
  seek: "Seek",
  playbackRate: "Speed",
  fullscreen: "Fullscreen",
  exitFullscreen: "Exit fullscreen"
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function options() {
  const container = document.createElement("div");
  return {
    assetId: 1,
    generation: 0,
    container,
    controlLabels: labels,
    onLoadedMetadata: vi.fn(),
    onFullscreenChange: vi.fn(),
    onError: vi.fn(),
    onNativeBounds: vi.fn(),
    onEscape: vi.fn(),
    onPointerActivity: vi.fn()
  };
}
function emit(event: MpvVideoEvent, call = 0) {
  const callback: (event: MpvVideoEvent) => void = api.openVideo.mock.calls[call][4];
  act(() => callback(event));
}

function mountSession(initial: SessionOptions) {
  const root = renderHook(() => {
    const [options, setOptions] = createSignal(initial);
    const session = createNativeVideoSession({
      get assetId() {
        return options().assetId;
      },
      get generation() {
        return options().generation;
      },
      get container() {
        return options().container;
      },
      get layoutRoot() {
        return options().layoutRoot;
      },
      get controlLabels() {
        return options().controlLabels;
      },
      onError: () => options().onError(),
      onLoadedMetadata: (value) => options().onLoadedMetadata(value),
      onFullscreenChange: (value) => options().onFullscreenChange(value),
      onNativeBounds: (value) => options().onNativeBounds?.(value),
      onEscape: () => options().onEscape?.(),
      onPointerActivity: () => options().onPointerActivity?.()
    });
    return { session, setOptions };
  });
  act(() => {});
  return {
    result: root.result.session,
    rerender: (next: Partial<SessionOptions>) =>
      act(() => root.result.setOptions((current) => ({ ...current, ...next }))),
    unmount: root.cleanup
  };
}

describe("createNativeVideoSession", () => {
  beforeEach(() => {
    for (const mock of Object.values(api)) mock.mockReset().mockResolvedValue(undefined);
    let request = 0;
    api.beginVideoOpen.mockImplementation(async () => ++request);
    api.openVideo.mockImplementation(async (_asset, request) => request * 10);
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("accepts early Escape only for the returned session and drops it after disposal", async () => {
    const open = deferred<number>();
    api.openVideo.mockReturnValueOnce(open.promise);
    const opts = options();
    const { unmount } = mountSession(opts);
    await waitFor(() => expect(api.openVideo).toHaveBeenCalledOnce());
    emit({ session_id: 10, type: "escape" });
    expect(opts.onEscape).not.toHaveBeenCalled();
    await act(async () => open.resolve(10));
    expect(opts.onEscape).toHaveBeenCalledOnce();
    emit({ session_id: 99, type: "escape" });
    expect(opts.onEscape).toHaveBeenCalledOnce();
    unmount();
    emit({ session_id: 10, type: "escape" });
    expect(opts.onEscape).toHaveBeenCalledOnce();
  });

  it("cancels a reservation that arrives after unmount without opening media", async () => {
    const reservation = deferred<number>();
    api.beginVideoOpen.mockReturnValue(reservation.promise);
    const { unmount } = mountSession(options());
    unmount();
    await act(async () => reservation.resolve(8));
    expect(api.cancelVideoOpen).toHaveBeenCalledWith(8);
    expect(api.openVideo).not.toHaveBeenCalled();
  });

  it("cancels a pending open and closes its late committed session", async () => {
    const open = deferred<number>();
    api.openVideo.mockReturnValue(open.promise);
    const opts = options();
    const { unmount } = mountSession(opts);
    await waitFor(() => expect(api.openVideo).toHaveBeenCalledOnce());
    unmount();
    expect(api.cancelVideoOpen).toHaveBeenCalledWith(1);
    await act(async () => open.resolve(10));
    expect(api.closeVideo).toHaveBeenCalledWith(10);
    expect(opts.onError).not.toHaveBeenCalled();
  });

  it("ignores old errors and bounds acknowledgements during A to B to A navigation", async () => {
    const firstBounds = deferred<void>();
    api.setVideoBounds.mockReturnValueOnce(firstBounds.promise);
    const opts = options();
    const { result, rerender } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    rerender({ assetId: 2, generation: 1 });
    await waitFor(() => expect(result.sessionId).toBe(20));
    rerender({ assetId: 1, generation: 2 });
    await waitFor(() => expect(result.sessionId).toBe(30));
    opts.onNativeBounds.mockClear();
    emit({ session_id: 10, type: "error", message: "old failure" });
    await act(async () => firstBounds.resolve());
    expect(opts.onError).not.toHaveBeenCalled();
    expect(opts.onNativeBounds).not.toHaveBeenCalled();
    emit({ session_id: 30, type: "pointerActivity" }, 2);
    expect(opts.onPointerActivity).toHaveBeenCalledOnce();
  });

  it("resets the lightbox fullscreen layout when replacing a fullscreen session", async () => {
    const opts = options();
    const { result, rerender } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    emit({ session_id: 10, type: "fullscreen", fullscreen: true });
    expect(opts.onFullscreenChange).toHaveBeenLastCalledWith(true);
    rerender({ ...opts, assetId: 2, generation: 1 });
    await waitFor(() => expect(result.sessionId).toBe(20));
    expect(opts.onFullscreenChange).toHaveBeenLastCalledWith(false);
    expect(result.fullscreen).toBe(false);
  });

  it("updates labels and callbacks without restarting playback", async () => {
    const opts = options();
    const { result, rerender } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    const onLoadedMetadata = vi.fn();
    rerender({ ...opts, onLoadedMetadata, controlLabels: { ...labels, play: "Odtwórz" } });
    await waitFor(() =>
      expect(api.setVideoControlLabels).toHaveBeenLastCalledWith(10, { ...labels, play: "Odtwórz" })
    );
    emit({ session_id: 10, type: "metadata", duration: 5, width: 320, height: 180 });
    expect(onLoadedMetadata).toHaveBeenCalledWith({ width: 320, height: 180 });
    expect(api.openVideo).toHaveBeenCalledOnce();
  });

  it("coalesces resize work and acknowledges only completed native placement", async () => {
    const bounds = deferred<void>();
    api.setVideoBounds.mockReturnValueOnce(bounds.promise);
    const opts = options();
    const rect = vi.spyOn(opts.container, "getBoundingClientRect");
    rect.mockReturnValue(new DOMRect(0, 0, 100, 100));
    const { result } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    for (const width of [200, 300, 400]) {
      rect.mockReturnValue(new DOMRect(0, 0, width, 100));
      act(() => window.dispatchEvent(new Event("resize")));
      await act(async () => new Promise(requestAnimationFrame));
    }
    expect(api.setVideoBounds).toHaveBeenCalledOnce();
    expect(opts.onNativeBounds).not.toHaveBeenCalled();
    await act(async () => bounds.resolve());
    expect(api.setVideoBounds).toHaveBeenCalledTimes(2);
    expect(api.setVideoBounds).toHaveBeenLastCalledWith(10, {
      x: 0,
      y: 0,
      width: 400,
      height: 100
    });
  });

  it("republishes moved bounds when the layout root resizes without restarting playback", async () => {
    const observers: Array<{
      target: Element;
      fire: () => void;
      disconnect: Mock<() => void>;
    }> = [];
    const OriginalObserver = globalThis.ResizeObserver;
    onTestFinished(() => {
      globalThis.ResizeObserver = OriginalObserver;
    });
    globalThis.ResizeObserver = class {
      private entries: typeof observers = [];
      constructor(private callback: () => void) {}
      observe(target: Element) {
        const entry = { target, fire: () => this.callback(), disconnect: vi.fn() };
        this.entries.push(entry);
        observers.push(entry);
      }
      unobserve() {}
      disconnect() {
        for (const entry of this.entries) entry.disconnect();
      }
    } as unknown as typeof ResizeObserver;
    const opts = options();
    const first = document.createElement("div");
    const rect = vi.spyOn(opts.container, "getBoundingClientRect");
    rect.mockReturnValue(new DOMRect(300, 150, 400, 300));
    const { result, rerender, unmount } = mountSession({ ...opts, layoutRoot: first });
    await waitFor(() => expect(result.sessionId).toBe(10));
    await waitFor(() => expect(opts.onNativeBounds).toHaveBeenCalled());
    const fire = (target: Element) =>
      act(() => {
        for (const entry of observers) if (entry.target === target) entry.fire();
      });
    rect.mockReturnValue(new DOMRect(130, 150, 400, 300));
    fire(first);
    await act(async () => new Promise(requestAnimationFrame));
    const moved = { x: 130, y: 150, width: 400, height: 300 };
    expect(api.setVideoBounds).toHaveBeenLastCalledWith(10, moved);
    expect(opts.onNativeBounds).toHaveBeenLastCalledWith(moved);

    const second = document.createElement("div");
    rerender({ layoutRoot: second });
    expect(observers.find((entry) => entry.target === first)?.disconnect).toHaveBeenCalledOnce();
    rect.mockReturnValue(new DOMRect(0, 150, 400, 300));
    fire(second);
    await act(async () => new Promise(requestAnimationFrame));
    expect(api.setVideoBounds).toHaveBeenLastCalledWith(10, { ...moved, x: 0 });
    expect(api.openVideo).toHaveBeenCalledOnce();

    unmount();
    expect(observers.find((entry) => entry.target === second)?.disconnect).toHaveBeenCalledOnce();
    const calls = api.setVideoBounds.mock.calls.length;
    fire(second);
    await act(async () => new Promise(requestAnimationFrame));
    expect(api.setVideoBounds).toHaveBeenCalledTimes(calls);
  });

  it("stops and reports fatal placement failures, but preserves playback after control failures", async () => {
    const opts = options();
    const { result } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    emit({ session_id: 10, type: "controlError", message: "seek rejected" });
    expect(result.controlError).toBe("seek rejected");
    expect(api.closeVideo).not.toHaveBeenCalled();
    act(() => result.dismissControlError());
    expect(result.controlError).toBeNull();
    vi.spyOn(console, "error").mockImplementation(() => {});
    api.setVideoBounds.mockRejectedValueOnce(new Error("surface failed"));
    act(() => window.dispatchEvent(new Event("resize")));
    await waitFor(() => expect(opts.onError).toHaveBeenCalledOnce());
    expect(api.closeVideo).toHaveBeenCalledWith(10);
  });

  it("keeps queued playback controls on their original session and ignores retired failures", async () => {
    const opts = options();
    const { result, rerender } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    const pending = deferred<void>();
    api.controlVideo.mockReturnValueOnce(pending.promise);
    const first = result.sendPlaybackControl({ type: "togglePause" });
    const second = result.sendPlaybackControl({ type: "seekRelative", seconds: 5 });
    await waitFor(() => expect(api.controlVideo).toHaveBeenCalledOnce());
    rerender({ ...opts, assetId: 2 });
    await waitFor(() => expect(result.sessionId).toBe(20));
    await act(async () => {
      pending.reject(new Error("retired session"));
      await Promise.all([first, second]);
    });
    expect(api.controlVideo).toHaveBeenCalledOnce();
    expect(result.controlError).toBeNull();
    api.controlVideo.mockRejectedValueOnce(new Error("seek failed"));
    await act(async () => result.sendPlaybackControl({ type: "seekRelative", seconds: -5 }));
    expect(api.controlVideo).toHaveBeenLastCalledWith(20, { type: "seekRelative", seconds: -5 });
    expect(result.controlError).toBe("seek failed");
    expect(opts.onError).not.toHaveBeenCalled();
  });

  it("serializes fullscreen toggles and reports rejection without optimistic UI state", async () => {
    const opts = options();
    const { result } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    await act(async () => {
      await Promise.all([result.toggleFullscreen(), result.toggleFullscreen()]);
    });
    expect(api.controlVideo.mock.calls).toEqual([
      [10, { type: "toggleFullscreen" }],
      [10, { type: "toggleFullscreen" }]
    ]);
    api.controlVideo.mockRejectedValueOnce(new Error("window manager failed"));
    await act(async () => result.toggleFullscreen());
    expect(result.fullscreen).toBe(false);
    expect(result.controlError).toBe("window manager failed");
  });
  it("buffers only the latest early event of each type and accepts the returned session", async () => {
    const open = deferred<number>();
    api.openVideo.mockReturnValueOnce(open.promise);
    const opts = options();
    const { result } = mountSession(opts);
    await waitFor(() => expect(api.openVideo).toHaveBeenCalledOnce());
    for (let width = 1; width <= 1000; width++) {
      emit({ session_id: 10, type: "metadata", width, height: 180, duration: 5 });
    }
    emit({ session_id: 99, type: "error", message: "another session" });
    emit({ session_id: 10, type: "fullscreen", fullscreen: true });
    expect(opts.onLoadedMetadata).not.toHaveBeenCalled();
    await act(async () => open.resolve(10));
    expect(opts.onLoadedMetadata).toHaveBeenCalledExactlyOnceWith({ width: 1000, height: 180 });
    expect(opts.onError).not.toHaveBeenCalled();
    expect(result.fullscreen).toBe(true);
    emit({ session_id: 99, type: "fullscreen", fullscreen: false });
    expect(result.fullscreen).toBe(true);
  });

  it("disposes observers and queued frames and rejects all events after cleanup", async () => {
    const disconnect = vi.fn();
    const OriginalObserver = globalThis.ResizeObserver;
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect = disconnect;
    };
    const opts = options();
    const { unmount } = mountSession(opts);
    await waitFor(() => expect(api.setVideoBounds).toHaveBeenCalledOnce());
    act(() => window.dispatchEvent(new Event("resize")));
    unmount();
    emit({ session_id: 10, type: "pointerActivity" });
    emit({ session_id: 10, type: "error", message: "late" });
    await act(async () => new Promise(requestAnimationFrame));
    expect(api.setVideoBounds).toHaveBeenCalledOnce();
    expect(disconnect).toHaveBeenCalledOnce();
    expect(opts.onError).not.toHaveBeenCalled();
    expect(opts.onPointerActivity).not.toHaveBeenCalled();
    globalThis.ResizeObserver = OriginalObserver;
  });

  it("ignores label failures from a retired session and reports current failures", async () => {
    const labelsRequest = deferred<void>();
    api.setVideoControlLabels.mockReturnValueOnce(labelsRequest.promise);
    const opts = options();
    const { result, rerender } = mountSession(opts);
    await waitFor(() => expect(result.sessionId).toBe(10));
    rerender({ assetId: 2 });
    await waitFor(() => expect(result.sessionId).toBe(20));
    await act(async () => labelsRequest.reject(Error("retired labels")));
    expect(result.controlError).toBeNull();
    api.setVideoControlLabels.mockRejectedValueOnce(Error("label failure"));
    rerender({ controlLabels: { ...labels, play: "Odtwórz" } });
    await waitFor(() => expect(result.controlError).toBe("label failure"));
    expect(api.openVideo).toHaveBeenCalledTimes(2);
  });
});
