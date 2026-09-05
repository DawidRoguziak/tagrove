import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useNativeVideoSession } from "../useNativeVideoSession";
import type { MpvVideoEvent, NativeVideoControlLabels } from "../mpvVideoTypes";

const api = vi.hoisted(() => ({
  beginVideoOpen: vi.fn(),
  cancelVideoOpen: vi.fn(),
  openVideo: vi.fn(),
  closeVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  setVideoControlLabels: vi.fn()
}));
vi.mock("../../../api", () => api);
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
    onPointerActivity: vi.fn()
  };
}
function emit(event: MpvVideoEvent, call = 0) {
  const callback: (event: MpvVideoEvent) => void = api.openVideo.mock.calls[call][4];
  act(() => callback(event));
}

describe("useNativeVideoSession", () => {
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

  it("cancels a reservation that arrives after unmount without opening media", async () => {
    const reservation = deferred<number>();
    api.beginVideoOpen.mockReturnValue(reservation.promise);
    const { unmount } = renderHook(() => useNativeVideoSession(options()));
    unmount();
    await act(async () => reservation.resolve(8));
    expect(api.cancelVideoOpen).toHaveBeenCalledWith(8);
    expect(api.openVideo).not.toHaveBeenCalled();
  });

  it("cancels a pending open and closes its late committed session", async () => {
    const open = deferred<number>();
    api.openVideo.mockReturnValue(open.promise);
    const opts = options();
    const { unmount } = renderHook(() => useNativeVideoSession(opts));
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
    const { result, rerender } = renderHook(
      ({ assetId, generation }) => useNativeVideoSession({ ...opts, assetId, generation }),
      { initialProps: { assetId: 1, generation: 0 } }
    );
    await waitFor(() => expect(result.current.sessionId).toBe(10));
    rerender({ assetId: 2, generation: 1 });
    await waitFor(() => expect(result.current.sessionId).toBe(20));
    rerender({ assetId: 1, generation: 2 });
    await waitFor(() => expect(result.current.sessionId).toBe(30));
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
    const { result, rerender } = renderHook((props) => useNativeVideoSession(props), {
      initialProps: opts
    });
    await waitFor(() => expect(result.current.sessionId).toBe(10));
    emit({ session_id: 10, type: "fullscreen", fullscreen: true });
    expect(opts.onFullscreenChange).toHaveBeenLastCalledWith(true);
    rerender({ ...opts, assetId: 2, generation: 1 });
    await waitFor(() => expect(result.current.sessionId).toBe(20));
    expect(opts.onFullscreenChange).toHaveBeenLastCalledWith(false);
    expect(result.current.fullscreen).toBe(false);
  });

  it("updates labels and callbacks without restarting playback", async () => {
    const opts = options();
    const { result, rerender } = renderHook((props) => useNativeVideoSession(props), {
      initialProps: opts
    });
    await waitFor(() => expect(result.current.sessionId).toBe(10));
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
    const { result } = renderHook(() => useNativeVideoSession(opts));
    await waitFor(() => expect(result.current.sessionId).toBe(10));
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

  it("stops and reports fatal placement failures, but preserves playback after control failures", async () => {
    const opts = options();
    const { result } = renderHook(() => useNativeVideoSession(opts));
    await waitFor(() => expect(result.current.sessionId).toBe(10));
    emit({ session_id: 10, type: "controlError", message: "seek rejected" });
    expect(result.current.controlError).toBe("seek rejected");
    expect(api.closeVideo).not.toHaveBeenCalled();
    act(() => result.current.dismissControlError());
    expect(result.current.controlError).toBeNull();
    vi.spyOn(console, "error").mockImplementation(() => {});
    api.setVideoBounds.mockRejectedValueOnce(new Error("surface failed"));
    act(() => window.dispatchEvent(new Event("resize")));
    await waitFor(() => expect(opts.onError).toHaveBeenCalledOnce());
    expect(api.closeVideo).toHaveBeenCalledWith(10);
  });

  it("serializes fullscreen toggles and reports rejection without optimistic UI state", async () => {
    const opts = options();
    const { result } = renderHook(() => useNativeVideoSession(opts));
    await waitFor(() => expect(result.current.sessionId).toBe(10));
    await act(async () => {
      await Promise.all([result.current.toggleFullscreen(), result.current.toggleFullscreen()]);
    });
    expect(api.controlVideo.mock.calls).toEqual([
      [10, { type: "toggleFullscreen" }],
      [10, { type: "toggleFullscreen" }]
    ]);
    api.controlVideo.mockRejectedValueOnce(new Error("window manager failed"));
    await act(async () => result.current.toggleFullscreen());
    expect(result.current.fullscreen).toBe(false);
    expect(result.current.controlError).toBe("window manager failed");
  });
});
