import { createEffect, createMemo, createSignal, onCleanup, onSettled } from "solid-js";
import type { SelectedAsset } from "../types";
import type { JSX } from "@solidjs/web";
import {
  calculateFittedMediaSize,
  clampPan,
  clampZoomFactor,
  getNextZoomFactor,
  type MediaDimensions
} from "./lightboxPanZoomService";

import type { LightboxVideoPlayerHandle } from "../video/LightboxVideoPlayer";

type Pointer = PointerEvent & { currentTarget: HTMLDivElement };
export function createImageControls(options: {
  selected: () => SelectedAsset;
  active: () => boolean;
  keyboardEnabled: () => boolean;
  close: () => void;
  previous: () => void;
  next: () => void;
  enterFullscreen: () => void;
}) {
  let videoPlayer: LightboxVideoPlayerHandle | null = null;
  let shell: HTMLDivElement | undefined, image: HTMLImageElement | undefined;
  const [viewport, setViewport] = createSignal<HTMLDivElement>();
  const [size, setSize] = createSignal<MediaDimensions>({ width: 0, height: 0 });
  const [intrinsic, setIntrinsic] = createSignal<MediaDimensions>({ width: 0, height: 0 });
  const fitted = createMemo(() => calculateFittedMediaSize(intrinsic(), size()));
  const [zoomed, setZoomed] = createSignal(false),
    [dragging, setDragging] = createSignal(false);
  const [fullscreen, setFullscreen] = createSignal(false);
  let zoom = 1,
    pan = { x: 0, y: 0 },
    frame: number | undefined;
  let suppressClick = 0,
    suppressClose = 0;
  let drag: {
    pointerId: number;
    target: HTMLDivElement;
    captured: boolean;
    x: number;
    y: number;
    origin: { x: number; y: number };
    moved: boolean;
  } | null = null;
  const isImage = () => options.selected().kind !== "video";
  function paint() {
    if (frame !== undefined) return;
    frame = requestAnimationFrame(() => {
      frame = undefined;
      image?.style.setProperty("--lightbox-image-zoom", String(zoom));
      if (image) image.style.transform = `translate(${pan.x}px, ${pan.y}px)`;
    });
  }
  function reset() {
    zoom = 1;
    pan = { x: 0, y: 0 };
    setZoomed(false);
    paint();
  }
  function updateZoom(next: number, anchor?: { x: number; y: number }) {
    const el = viewport();
    if (!image || !el || !isImage()) return;
    const display = fitted(),
      box = size();
    if (!display.width || !display.height || !box.width || !box.height) return;
    const rect = el.getBoundingClientRect(),
      value = clampZoomFactor(next);
    const x = (anchor?.x ?? rect.left + rect.width / 2) - rect.left - rect.width / 2;
    const y = (anchor?.y ?? rect.top + rect.height / 2) - rect.top - rect.height / 2;
    pan = clampPan(
      { x: x - ((x - pan.x) / zoom) * value, y: y - ((y - pan.y) / zoom) * value },
      value,
      display,
      box
    );
    zoom = value;
    setZoomed(zoom > 1);
    paint();
  }
  function finishDrag() {
    const previous = drag;
    if (!previous) return;
    drag = null;
    setDragging(false);
    suppressClose = Date.now() + 240;
    if (previous.moved) suppressClick = Date.now() + 120;
    try {
      if (previous.target.hasPointerCapture(previous.pointerId))
        previous.target.releasePointerCapture(previous.pointerId);
    } catch {
      /* Browser may already have released capture. */
    }
  }
  function move(event: PointerEvent) {
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.preventDefault();
    const dx = event.clientX - drag.x,
      dy = event.clientY - drag.y;
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) drag.moved = true;
    pan = clampPan({ x: drag.origin.x + dx, y: drag.origin.y + dy }, zoom, fitted(), size());
    paint();
  }
  function end(event: PointerEvent) {
    if (drag?.pointerId === event.pointerId) finishDrag();
  }
  async function toggleFullscreen() {
    if (!isImage()) {
      await videoPlayer?.toggleFullscreen();
      return;
    }
    if (!shell) return;
    try {
      if (document.fullscreenElement === shell) {
        await document.exitFullscreen();
        return;
      }
      if (document.fullscreenElement) await document.exitFullscreen();
      options.enterFullscreen();
      await shell.requestFullscreen();
    } catch {
      /* Fullscreen may be unavailable in the current window. */
    }
  }
  const dimensions = createMemo<{ id: number; width: number | null; height: number | null }>(
    (previous) => {
      const asset = options.selected();
      return previous?.id === asset.id &&
        previous.width === asset.width &&
        previous.height === asset.height
        ? previous
        : { id: asset.id, width: asset.width, height: asset.height };
    }
  );
  createEffect(dimensions, (asset) => {
    finishDrag();
    reset();
    suppressClick = 0;
    setIntrinsic({ width: asset.width ?? 0, height: asset.height ?? 0 });
  });
  createEffect(
    () => ({ viewport: viewport(), active: options.active() }),
    ({ viewport: el, active }) => {
      if (!el || !active) return;
      const measure = () => setSize({ width: el.clientWidth, height: el.clientHeight });
      const observer = new ResizeObserver(measure);
      observer.observe(el);
      measure();
      const wheel = (event: WheelEvent) => {
        if (!isImage() || !event.deltaY) return;
        event.preventDefault();
        suppressClose = Date.now() + 360;
        const multiplier =
          event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? Math.max(1, el.clientHeight) : 1;
        const delta = Math.max(-100, Math.min(100, event.deltaY * multiplier));
        updateZoom(zoom * Math.exp(-delta * 0.002), { x: event.clientX, y: event.clientY });
      };
      el.addEventListener("wheel", wheel, { passive: false });
      window.addEventListener("resize", measure);
      return () => {
        observer.disconnect();
        el.removeEventListener("wheel", wheel);
        window.removeEventListener("resize", measure);
      };
    }
  );
  createEffect(
    () => ({ fitted: fitted(), size: size() }),
    ({ fitted, size }) => {
      pan = clampPan(pan, zoom, fitted, size);
      paint();
    }
  );
  createEffect(options.active, (active) => {
    if (!active) {
      finishDrag();
      return;
    }
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", finishDrag);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", finishDrag);
    };
  });
  createEffect(isImage, (imageSelected) => {
    if (imageSelected) setFullscreen(!!shell && document.fullscreenElement === shell);
  });
  onSettled(() => {
    const changed = () => {
      if (isImage()) setFullscreen(!!shell && document.fullscreenElement === shell);
    };
    const keyboard = (event: KeyboardEvent) => {
      if (!options.active() || !options.keyboardEnabled() || event.defaultPrevented) return;
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (!isImage() && fullscreen() && event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        if (!event.repeat) void toggleFullscreen();
        return;
      }
      if (target?.matches("input,textarea") || target?.isContentEditable) return;
      if (
        !isImage() &&
        target?.closest("[data-lightbox-video-player]") &&
        event.key !== "f" &&
        event.key !== "F"
      )
        return;
      const actions: Record<string, () => void> = {
        ArrowLeft: options.previous,
        ArrowRight: options.next,
        f: () => {
          void toggleFullscreen();
        },
        F: () => {
          void toggleFullscreen();
        },
        ...(isImage()
          ? {
              "0": reset,
              "+": () => updateZoom(getNextZoomFactor(zoom, "in")),
              "=": () => updateZoom(getNextZoomFactor(zoom, "in")),
              "-": () => updateZoom(getNextZoomFactor(zoom, "out"))
            }
          : {})
      };
      const action = actions[event.key];
      if (action) {
        event.preventDefault();
        action();
      }
    };
    document.addEventListener("fullscreenchange", changed);
    window.addEventListener("keydown", keyboard, true);
    return () => {
      document.removeEventListener("fullscreenchange", changed);
      window.removeEventListener("keydown", keyboard, true);
    };
  });
  onCleanup(() => {
    finishDrag();
    if (frame !== undefined) cancelAnimationFrame(frame);
  });
  return {
    shell: () => shell,
    shellRef: (el: HTMLDivElement) => {
      shell = el;
    },
    viewport: () => viewport(),
    viewportRef: setViewport,
    videoPlayerRef: (handle: LightboxVideoPlayerHandle | null) => {
      videoPlayer = handle;
    },
    onVideoLoadedMetadata: (dimensions: MediaDimensions) => setIntrinsic(dimensions),
    onVideoFullscreenChange: (value: boolean) => {
      setFullscreen(value);
      if (value) options.enterFullscreen();
    },
    imageRef: (el: HTMLImageElement) => {
      image = el;
      paint();
    },
    get isZoomed() {
      return zoomed();
    },
    get isDragging() {
      return dragging();
    },
    get isFullscreen() {
      return fullscreen();
    },
    get mediaDisplaySize() {
      return fitted();
    },
    get zoomFactor() {
      return zoom;
    },
    resetZoom: reset,
    toggleFullscreen,
    tryClose() {
      if (!drag && Date.now() >= suppressClose) options.close();
    },
    onLoad: ((event) => {
      image = event.currentTarget;
      setIntrinsic({ width: image.naturalWidth, height: image.naturalHeight });
      paint();
    }) satisfies JSX.EventHandler<HTMLImageElement, Event>,
    onClick: ((event) => {
      if (!isImage() || Date.now() < suppressClick) return;
      suppressClose = Date.now() + 360;
      updateZoom(getNextZoomFactor(zoom, "in"), { x: event.clientX, y: event.clientY });
    }) satisfies JSX.EventHandler<HTMLImageElement, MouseEvent>,
    onPointerDown(event: Pointer) {
      if (!isImage() || !event.isPrimary || event.button !== 0 || zoom <= 1) return;
      event.preventDefault();
      suppressClose = Date.now() + 500;
      let captured = false;
      try {
        event.currentTarget.setPointerCapture(event.pointerId);
        captured = event.currentTarget.hasPointerCapture(event.pointerId);
      } catch {
        /* Window listeners cover missing capture. */
      }
      drag = {
        pointerId: event.pointerId,
        target: event.currentTarget,
        captured,
        x: event.clientX,
        y: event.clientY,
        origin: { ...pan },
        moved: false
      };
      setDragging(true);
    },
    onPointerEnd: end
  };
}
