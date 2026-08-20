import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useLightboxVideoSource } from "../useLightboxVideoSource";

const apiMocks = vi.hoisted(() => ({
  getVideoStreamUrl: vi.fn()
}));

vi.mock("../../../../api", () => ({
  getVideoStreamUrl: apiMocks.getVideoStreamUrl
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("useLightboxVideoSource", () => {
  let consoleError: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    apiMocks.getVideoStreamUrl.mockReset();
    consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleError.mockRestore();
  });

  it("loads a stream URL for the selected video", async () => {
    apiMocks.getVideoStreamUrl.mockResolvedValueOnce("http://video/7");
    const { result } = renderHook(() => useLightboxVideoSource(7, "/media/a.mp4", true));

    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.src).toBe("http://video/7"));
    expect(result.current).toEqual({ src: "http://video/7", failed: false, loading: false });
    expect(apiMocks.getVideoStreamUrl).toHaveBeenCalledWith(7);
  });

  it("does not request a URL for non-video media", () => {
    const { result } = renderHook(() => useLightboxVideoSource(7, "/media/a.jpg", false));

    expect(result.current).toEqual({ src: null, failed: false, loading: false });
    expect(apiMocks.getVideoStreamUrl).not.toHaveBeenCalled();
  });

  it("reports a current IPC failure", async () => {
    const error = new Error("unavailable");
    apiMocks.getVideoStreamUrl.mockRejectedValueOnce(error);
    const { result } = renderHook(() => useLightboxVideoSource(7, "/media/a.mp4", true));

    await waitFor(() => expect(result.current.failed).toBe(true));
    expect(result.current.src).toBeNull();
    expect(consoleError).toHaveBeenCalledWith(
      "[lightbox] Failed to resolve video stream URL",
      expect.objectContaining({ assetId: 7, path: "/media/a.mp4", error })
    );
  });

  it("rejects an empty stream URL", async () => {
    apiMocks.getVideoStreamUrl.mockResolvedValueOnce("");
    const { result } = renderHook(() => useLightboxVideoSource(7, "/media/a.mp4", true));

    await waitFor(() => expect(result.current.failed).toBe(true));
    expect(result.current).toEqual({ src: null, failed: true, loading: false });
    expect(consoleError).toHaveBeenCalledWith("[lightbox] Video stream URL was empty", {
      assetId: 7,
      path: "/media/a.mp4"
    });
  });

  it("clears the old source and ignores a late response after navigation", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    apiMocks.getVideoStreamUrl
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(
      ({ id, path }) => useLightboxVideoSource(id, path, true),
      { initialProps: { id: 1, path: "/media/a.mp4" } }
    );

    rerender({ id: 2, path: "/media/b.mp4" });
    expect(result.current.src).toBeNull();
    await act(async () => second.resolve("http://video/2"));
    expect(result.current.src).toBe("http://video/2");
    await act(async () => first.resolve("http://video/1"));
    expect(result.current.src).toBe("http://video/2");
  });

  it("ignores a stale rejection after the next video loads", async () => {
    const first = deferred<string>();
    apiMocks.getVideoStreamUrl
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce("http://video/2");
    const { result, rerender } = renderHook(
      ({ id, path }) => useLightboxVideoSource(id, path, true),
      { initialProps: { id: 1, path: "/media/a.mp4" } }
    );

    rerender({ id: 2, path: "/media/b.mp4" });
    await waitFor(() => expect(result.current.src).toBe("http://video/2"));
    await act(async () => first.reject(new Error("late")));
    expect(result.current).toEqual({ src: "http://video/2", failed: false, loading: false });
    expect(consoleError).not.toHaveBeenCalled();
  });
});
