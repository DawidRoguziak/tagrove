import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { CSSProperties, MutableRefObject } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../types";
import { LightboxMediaStage } from "../LightboxMediaStage";
import type { LightboxVideoPlayerHandle } from "../LightboxVideoPlayer";
import { useLightboxImageControls } from "../useLightboxImageControls";

const videoPlayerMocks = vi.hoisted(() => ({
  activations: [] as Array<{
    assetId: number;
    generation: number;
    onError: () => void;
    style?: CSSProperties;
  }>
}));

vi.mock("../../../api", () => ({
  toMediaSrc: (path: string) => `media://${path}`
}));

vi.mock("../LightboxVideoPlayer", () => ({
  LightboxVideoPlayer: ({
    assetId,
    generation,
    onError,
    style
  }: {
    assetId: number;
    generation: number;
    onError: () => void;
    style?: CSSProperties;
  }) => {
    videoPlayerMocks.activations.push({ assetId, generation, onError, style });
    return <div data-testid="mock-video-player" data-asset-id={assetId} style={style} />;
  }
}));

function createVideo(id: number): SelectedAsset {
  return {
    id,
    file_name: `${id}.mp4`,
    preview_path: null,
    kind: "video",
    modified_at: id,
    width: 160,
    height: 90,
    duration_ms: 10_000,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    path: `C:/media/${id}.mp4`,
    size_bytes: 100,
    tags: []
  };
}

function stageProps(selected: SelectedAsset) {
  return {
    selected,
    mediaViewportRef: { current: null } as MutableRefObject<HTMLDivElement | null>,
    lightboxImageRef: { current: null } as MutableRefObject<HTMLImageElement | null>,
    lightboxVideoPlayerRef: { current: null } as MutableRefObject<LightboxVideoPlayerHandle | null>,
    isZoomed: false,
    mediaDisplaySize: { width: 160, height: 90 },
    isDragging: false,
    isFullscreen: false,
    onImageLoad: vi.fn(),
    onVideoLoadedMetadata: vi.fn(),
    onVideoFullscreenChange: vi.fn(),
    onImageClick: vi.fn(),
    onImagePointerDown: vi.fn(),
    onImagePointerMove: vi.fn(),
    onImagePointerEnd: vi.fn()
  };
}

describe("LightboxMediaStage", () => {
  beforeEach(() => {
    videoPlayerMocks.activations.length = 0;
  });

  afterEach(cleanup);

  it("fits video to the complete native player footprint", () => {
    render(<LightboxMediaStage {...stageProps(createVideo(1))} />);

    expect(screen.getByTestId("mock-video-player")).toHaveStyle({
      width: "160px",
      height: "90px"
    });
  });

  it("retries A after A to B to A and ignores a late error from the first activation", () => {
    const first = createVideo(1);
    const second = createVideo(2);
    const { rerender } = render(<LightboxMediaStage {...stageProps(first)} />);

    expect(screen.getByTestId("mock-video-player")).toHaveAttribute("data-asset-id", "1");
    const staleFirstError = videoPlayerMocks.activations.at(-1)!.onError;
    act(() => staleFirstError());
    expect(screen.getByTestId("lightbox-media-error")).toBeInTheDocument();

    rerender(<LightboxMediaStage {...stageProps(second)} />);
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    expect(screen.getByTestId("mock-video-player")).toHaveAttribute("data-asset-id", "2");

    rerender(<LightboxMediaStage {...stageProps(first)} />);
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    expect(screen.getByTestId("mock-video-player")).toHaveAttribute("data-asset-id", "1");
    const currentFirstError = videoPlayerMocks.activations.at(-1)!.onError;

    act(() => staleFirstError());
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    act(() => currentFirstError());
    expect(screen.getByTestId("lightbox-media-error")).toBeInTheDocument();
  });

  it("offers Retry when primary asset details fail", () => {
    const onRetryDetails = vi.fn();
    const selected = {
      ...createVideo(3),
      kind: "image" as const,
      file_name: "3.jpg",
      path: null
    };
    render(
      <LightboxMediaStage
        {...stageProps(selected)}
        detailsFailed
        onRetryDetails={onRetryDetails}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "Retry loading details" }));
    expect(onRetryDetails).toHaveBeenCalledTimes(1);
  });
});

function ImageStage({ selected }: { selected: SelectedAsset }) {
  const controls = useLightboxImageControls({
    selected,
    onClose: vi.fn(),
    onNavigatePrevious: vi.fn(),
    onNavigateNext: vi.fn(),
    onEnterFullscreen: vi.fn()
  });
  return (
    <LightboxMediaStage
      {...stageProps(selected)}
      {...controls}
      onImageLoad={controls.handleImageLoad}
      onImageClick={controls.handleImageClick}
      onImagePointerDown={controls.handleImagePointerDown}
      onImagePointerMove={controls.handleImagePointerMove}
      onImagePointerEnd={controls.handleImagePointerEnd}
    />
  );
}

describe("lightbox image zoom rendering", () => {
  const OriginalResizeObserver = globalThis.ResizeObserver;
  let viewportWidth: number;
  let viewportHeight: number;
  let resizeViewport: () => void;
  const portrait: SelectedAsset = {
    ...createVideo(10),
    kind: "image",
    file_name: "portrait.png",
    path: "C:/media/portrait.png",
    width: 1200,
    height: 6000
  };

  beforeEach(() => {
    vi.useFakeTimers();
    viewportWidth = 800;
    viewportHeight = 600;
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => viewportWidth);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(() => viewportHeight);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      () => new DOMRect(0, 0, viewportWidth, viewportHeight)
    );
    vi.stubGlobal("ResizeObserver", class implements ResizeObserver {
      constructor(callback: ResizeObserverCallback) {
        resizeViewport = () => callback([], this);
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    });
  });

  afterEach(() => {
    cleanup();
    vi.stubGlobal("ResizeObserver", OriginalResizeObserver);
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  function flushFrame() {
    act(() => vi.advanceTimersByTime(32));
  }

  function imageElement() {
    return screen.getByRole<HTMLImageElement>("img");
  }

  function zoomToTen() {
    // Ten 1.25x keyboard steps, then a wheel adjustment to exactly 10x.
    for (let step = 0; step < 10; step++) fireEvent.keyDown(window, { key: "+" });
    fireEvent.wheel(imageElement(), {
      deltaY: -Math.log(10 / 1.25 ** 10) / 0.002,
      clientX: 400,
      clientY: 300
    });
    flushFrame();
  }

  it.each(["image", "gif"] as const)("sizes a tall %s from the original and keeps zoom across rerenders", (kind) => {
    const selected = { ...portrait, kind };
    const { rerender } = render(<ImageStage selected={selected} />);
    zoomToTen();

    const image = imageElement();
    expect(image.style.width).toBe("calc(120px * var(--lightbox-image-zoom, 1))");
    expect(image.style.height).toBe("calc(600px * var(--lightbox-image-zoom, 1))");
    expect(Number(image.style.getPropertyValue("--lightbox-image-zoom"))).toBeCloseTo(10);
    expect(image.style.transform).toBe("translate(0px, 0px)");
    expect(image.style.maxWidth).toBe("none");
    expect(image.style.maxHeight).toBe("none");

    rerender(<ImageStage selected={{ ...selected, tags: ["edited"] }} />);
    flushFrame();
    expect(Number(image.style.getPropertyValue("--lightbox-image-zoom"))).toBeCloseTo(10);

    fireEvent.keyDown(window, { key: "0" });
    flushFrame();
    expect(image.style.getPropertyValue("--lightbox-image-zoom")).toBe("1");
    expect(image.style.transform).toBe("translate(0px, 0px)");
  });

  it("preserves the wheel anchor and clamps drag and resize pan to the zoomed bounds", () => {
    render(<ImageStage selected={portrait} />);
    zoomToTen();
    const image = imageElement();
    fireEvent.wheel(image, { deltaY: -Math.log(1.2) / 0.002, clientX: 450, clientY: 400 });
    flushFrame();
    const pan = image.style.transform.match(/translate\(([-\d.]+)px, ([-\d.]+)px\)/);
    expect(Number(pan?.[1])).toBeCloseTo(-10);
    expect(Number(pan?.[2])).toBeCloseTo(-20);

    // jsdom lacks PointerEvent; supply mouse coordinates and pointer identity.
    function pointerEvent(type: string, clientX: number, clientY: number) {
      const event = new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY });
      Object.defineProperty(event, "pointerId", { value: 1 });
      Object.defineProperty(event, "isPrimary", { value: true });
      return event;
    }
    fireEvent(image, pointerEvent("pointerdown", 400, 300));
    fireEvent(window, pointerEvent("pointermove", 10000, 10000));
    fireEvent(window, pointerEvent("pointerup", 10000, 10000));
    flushFrame();
    expect(image.style.transform).toBe("translate(320px, 3300px)");

    act(() => {
      viewportWidth = 1200;
      viewportHeight = 400;
      resizeViewport();
    });
    flushFrame();
    expect(image.style.height).toBe("calc(400px * var(--lightbox-image-zoom, 1))");
    expect(image.style.transform).toBe("translate(0px, 2200px)");
  });

  it("uses natural dimensions after load and resets on navigation", () => {
    const { rerender } = render(<ImageStage selected={portrait} />);
    const image = imageElement();
    Object.defineProperties(image, {
      naturalWidth: { value: 2400 },
      naturalHeight: { value: 6000 }
    });
    fireEvent.load(image);
    flushFrame();
    expect(image.style.width).toBe("calc(240px * var(--lightbox-image-zoom, 1))");
    zoomToTen();

    rerender(<ImageStage selected={{ ...portrait, id: 11, width: 1600, height: 1200 }} />);
    flushFrame();
    expect(image.style.width).toBe("calc(800px * var(--lightbox-image-zoom, 1))");
    expect(image.style.getPropertyValue("--lightbox-image-zoom")).toBe("1");
    expect(image.style.transform).toBe("translate(0px, 0px)");
  });

  it("coalesces zoom writes into one frame and cancels pending work on unmount", () => {
    const { unmount } = render(<ImageStage selected={portrait} />);
    flushFrame();
    const requestFrame = vi.spyOn(window, "requestAnimationFrame");
    const cancelFrame = vi.spyOn(window, "cancelAnimationFrame");
    for (let step = 0; step < 3; step++) fireEvent.keyDown(window, { key: "+" });
    expect(requestFrame).toHaveBeenCalledTimes(1);
    flushFrame();
    expect(Number(imageElement().style.getPropertyValue("--lightbox-image-zoom"))).toBe(1.25 ** 3);
    fireEvent.keyDown(window, { key: "+" });
    unmount();
    expect(cancelFrame).toHaveBeenCalledTimes(1);
  });
});
