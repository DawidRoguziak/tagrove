import { cleanup, fireEvent, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderSolid } from "../../../test/renderSolid";
import { settle as act } from "../../../test/settle";
import { createImageControls } from "../image/createImageControls";
import { LightboxMediaStage } from "../LightboxMediaStage";
import type { SelectedAsset } from "../types";
import { asset } from "./fixtures";
vi.mock("../../../platform/tauri/assets", () => ({
  toMediaSrc: (path: string) => `asset://${path}`
}));
function ImageStage(props: { selected: SelectedAsset }) {
  const controls = createImageControls({
    selected: () => props.selected,
    active: () => true,
    keyboardEnabled: () => true,
    close: vi.fn(),
    previous: vi.fn(),
    next: vi.fn(),
    enterFullscreen: vi.fn()
  });
  return (
    <LightboxMediaStage
      selected={props.selected}
      active
      detailsFailed={false}
      retryDetails={vi.fn()}
      controls={controls}
    />
  );
}
describe("lightbox image zoom rendering", () => {
  const OriginalResizeObserver = globalThis.ResizeObserver;
  let viewportWidth: number;
  let viewportHeight: number;
  let resizeViewport: () => void;
  const portrait: SelectedAsset = {
    ...asset(10),
    kind: "image",
    file_name: "portrait.png",
    path: "/synthetic/portrait.png",
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
    vi.stubGlobal(
      "ResizeObserver",
      class implements ResizeObserver {
        constructor(callback: ResizeObserverCallback) {
          resizeViewport = () => callback([], this);
        }
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    );
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

  it.each([
    "image",
    "gif"
  ] as const)("sizes a tall %s from the original and keeps zoom across rerenders", (kind) => {
    const selected = { ...portrait, kind };
    const { rerender } = renderSolid(ImageStage, { selected });
    const image = imageElement();
    zoomToTen();

    expect(imageElement()).toBe(image);
    expect(image.style.width).toBe("calc(120px * var(--lightbox-image-zoom, 1))");
    expect(image.style.height).toBe("calc(600px * var(--lightbox-image-zoom, 1))");
    expect(Number(image.style.getPropertyValue("--lightbox-image-zoom"))).toBeCloseTo(10);
    expect(image.style.transform).toBe("translate(0px, 0px)");
    expect(image.style.maxWidth).toBe("none");
    expect(image.style.maxHeight).toBe("none");

    rerender({ selected: { ...selected, tags: ["edited"] } });
    flushFrame();
    expect(imageElement()).toBe(image);
    expect(Number(image.style.getPropertyValue("--lightbox-image-zoom"))).toBeCloseTo(10);

    fireEvent.keyDown(window, { key: "0" });
    flushFrame();
    expect(image.style.getPropertyValue("--lightbox-image-zoom")).toBe("1");
    expect(image.style.transform).toBe("translate(0px, 0px)");
  });

  it("preserves the wheel anchor and clamps drag and resize pan to the zoomed bounds", () => {
    renderSolid(ImageStage, { selected: portrait });
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
    const { rerender } = renderSolid(ImageStage, { selected: portrait });
    const image = imageElement();
    Object.defineProperties(image, {
      naturalWidth: { value: 2400 },
      naturalHeight: { value: 6000 }
    });
    fireEvent.load(image);
    flushFrame();
    expect(image.style.width).toBe("calc(240px * var(--lightbox-image-zoom, 1))");
    zoomToTen();

    rerender({ selected: { ...portrait, id: 11, width: 1600, height: 1200 } });
    flushFrame();
    const nextImage = imageElement();
    expect(nextImage).not.toBe(image);
    expect(nextImage.style.width).toBe("calc(800px * var(--lightbox-image-zoom, 1))");
    expect(nextImage.style.getPropertyValue("--lightbox-image-zoom")).toBe("1");
    expect(nextImage.style.transform).toBe("translate(0px, 0px)");
  });

  it("coalesces zoom writes into one frame and cancels pending work on unmount", () => {
    const { unmount } = renderSolid(ImageStage, { selected: portrait });
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
