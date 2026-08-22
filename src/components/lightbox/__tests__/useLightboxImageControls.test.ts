import { act, renderHook } from "@testing-library/react";
import type { MouseEvent as ReactMouseEvent, SyntheticEvent, WheelEvent as ReactWheelEvent } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../types";
import { useLightboxImageControls } from "../useLightboxImageControls";

function createAsset(kind: SelectedAsset["kind"] = "image"): SelectedAsset {
  return {
    id: 1,
    file_name: "1.jpg",
    preview_path: null,
    kind,
    modified_at: 1,
    width: 1200,
    height: 800,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    path: "C:/library/1.jpg",
    size_bytes: 100,
    tags: []
  };
}

function setupViewport(node: HTMLDivElement) {
  Object.defineProperty(node, "clientWidth", {
    configurable: true,
    get: () => 800
  });
  Object.defineProperty(node, "clientHeight", {
    configurable: true,
    get: () => 600
  });
  node.getBoundingClientRect = () => ({
    x: 0,
    y: 0,
    width: 800,
    height: 600,
    top: 0,
    left: 0,
    right: 800,
    bottom: 600,
    toJSON: () => ({})
  });
}

function setupImageNaturalSize(image: HTMLImageElement) {
  Object.defineProperty(image, "naturalWidth", {
    configurable: true,
    get: () => 1600
  });
  Object.defineProperty(image, "naturalHeight", {
    configurable: true,
    get: () => 1200
  });
}

describe("useLightboxImageControls", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    Object.defineProperty(document, "fullscreenElement", {
      configurable: true,
      writable: true,
      value: null
    });
    Object.defineProperty(document, "exitFullscreen", {
      configurable: true,
      writable: true,
      value: vi.fn(async () => {
        Object.defineProperty(document, "fullscreenElement", {
          configurable: true,
          writable: true,
          value: null
        });
        document.dispatchEvent(new Event("fullscreenchange"));
      })
    });
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it("handles keyboard shortcuts and tracks fullscreen state", async () => {
    const onClose = vi.fn();
    const onNavigatePrevious = vi.fn();
    const onNavigateNext = vi.fn();
    const onEnterFullscreen = vi.fn();

    const { result } = renderHook(() =>
      useLightboxImageControls({
        selected: createAsset("image"),
        onClose,
        onNavigatePrevious,
        onNavigateNext,
        onEnterFullscreen
      })
    );

    const shell = document.createElement("div");
    const viewport = document.createElement("div");
    setupViewport(viewport);
    const image = document.createElement("img");
    setupImageNaturalSize(image);
    Object.defineProperty(shell, "requestFullscreen", {
      configurable: true,
      value: vi.fn(async () => {
        Object.defineProperty(document, "fullscreenElement", {
          configurable: true,
          writable: true,
          value: shell
        });
        document.dispatchEvent(new Event("fullscreenchange"));
      })
    });

    act(() => {
      result.current.lightboxShellRef.current = shell;
      result.current.mediaViewportRef.current = viewport;
      result.current.handleImageLoad({ currentTarget: image } as SyntheticEvent<HTMLImageElement>);
    });

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "+" }));
      await Promise.resolve();
    });

    expect(onNavigatePrevious).toHaveBeenCalledTimes(1);
    expect(onNavigateNext).toHaveBeenCalledTimes(1);
    expect(result.current.zoomLevel).toBeGreaterThan(1);

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "0" }));
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
      await Promise.resolve();
    });

    expect(result.current.zoomLevel).toBe(1);
    expect(onEnterFullscreen).toHaveBeenCalledTimes(1);
    expect((shell.requestFullscreen as ReturnType<typeof vi.fn>)).toHaveBeenCalledTimes(1);
    expect(result.current.isFullscreen).toBe(true);

    await act(async () => {
      await result.current.toggleFullscreen();
    });

    expect(result.current.isFullscreen).toBe(false);

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      await Promise.resolve();
    });

    expect(onClose).not.toHaveBeenCalled();
  });

  it("delegates video fullscreen through the local player handle", async () => {
    const toggleFullscreen = vi.fn(async () => {});
    const { result } = renderHook(() =>
      useLightboxImageControls({
        selected: createAsset("video"),
        onClose: vi.fn(),
        onNavigatePrevious: vi.fn(),
        onNavigateNext: vi.fn(),
        onEnterFullscreen: vi.fn()
      })
    );
    result.current.lightboxVideoPlayerRef.current = { toggleFullscreen };

    await act(async () => {
      await result.current.toggleFullscreen();
    });

    expect(toggleFullscreen).toHaveBeenCalledTimes(1);
  });

  it("zooms with wheel and blocks backdrop close during suppression window", async () => {
    const onClose = vi.fn();

    const { result } = renderHook(() =>
      useLightboxImageControls({
        selected: createAsset("image"),
        onClose,
        onNavigatePrevious: vi.fn(),
        onNavigateNext: vi.fn(),
        onEnterFullscreen: vi.fn()
      })
    );

    const viewport = document.createElement("div");
    setupViewport(viewport);
    const image = document.createElement("img");
    setupImageNaturalSize(image);

    act(() => {
      result.current.mediaViewportRef.current = viewport;
      result.current.handleImageLoad({ currentTarget: image } as SyntheticEvent<HTMLImageElement>);
    });

    const preventDefault = vi.fn();
    act(() => {
      result.current.handleImageWheel({
        deltaY: -120,
        clientX: 250,
        clientY: 200,
        preventDefault
      } as unknown as ReactWheelEvent<HTMLImageElement>);
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(result.current.zoomLevel).toBeGreaterThan(1);

    act(() => {
      result.current.tryCloseLightbox();
    });
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(500);
      await Promise.resolve();
    });

    act(() => {
      result.current.tryCloseLightbox();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("starts drag pan only after zooming and ends drag on mouseup", async () => {
    const onClose = vi.fn();

    const { result } = renderHook(() =>
      useLightboxImageControls({
        selected: createAsset("image"),
        onClose,
        onNavigatePrevious: vi.fn(),
        onNavigateNext: vi.fn(),
        onEnterFullscreen: vi.fn()
      })
    );

    const viewport = document.createElement("div");
    setupViewport(viewport);
    const image = document.createElement("img");
    setupImageNaturalSize(image);

    act(() => {
      result.current.mediaViewportRef.current = viewport;
      result.current.handleImageLoad({ currentTarget: image } as SyntheticEvent<HTMLImageElement>);
      result.current.handleImageDoubleClick();
    });

    const preventDefault = vi.fn();
    act(() => {
      result.current.handleImageMouseDown({
        clientX: 300,
        clientY: 200,
        preventDefault
      } as unknown as ReactMouseEvent<HTMLImageElement>);
    });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(result.current.isDragging).toBe(true);

    await act(async () => {
      window.dispatchEvent(new MouseEvent("mousemove", { clientX: 340, clientY: 235 }));
      vi.advanceTimersByTime(32);
      await Promise.resolve();
    });

    expect(result.current.zoomLevel).toBe(2);

    act(() => {
      window.dispatchEvent(new MouseEvent("mouseup"));
    });

    expect(result.current.isDragging).toBe(false);

    act(() => {
      result.current.tryCloseLightbox();
    });
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(260);
      await Promise.resolve();
    });

    act(() => {
      result.current.tryCloseLightbox();
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
