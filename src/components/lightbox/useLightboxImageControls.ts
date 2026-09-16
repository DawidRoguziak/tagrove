import type {
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
  SyntheticEvent
} from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SelectedAsset } from "../../types";
import { useLightboxKeyboardShortcuts } from "./hooks/useLightboxKeyboardShortcuts";
import { useLightboxViewportSize } from "./hooks/useLightboxViewportSize";
import type { LightboxVideoPlayerHandle } from "./LightboxVideoPlayer";
import {
  calculateFittedMediaSize,
  clampPan,
  clampZoomFactor,
  getDoubleClickZoomFactor,
  getNextZoomFactor,
  MIN_ZOOM_FACTOR,
  type MediaDimensions
} from "./services/lightboxPanZoomService";

interface UseLightboxImageControlsOptions {
  keyboardShortcutsEnabled?: boolean;
  keyboardShortcutsSuspended?: boolean;
  selected: SelectedAsset | null;
  onClose: () => void;
  onNavigatePrevious: () => void;
  onNavigateNext: () => void;
  onEnterFullscreen: () => void;
}

interface ZoomWheelEvent {
  deltaY: number;
  deltaMode?: number;
  clientX: number;
  clientY: number;
  preventDefault: () => void;
}

const DRAG_CLICK_SUPPRESSION_THRESHOLD_PX = 3;
const WHEEL_LINE_HEIGHT_PX = 16;
const MAX_WHEEL_DELTA_PX = 100;
const WHEEL_ZOOM_SENSITIVITY = 0.002;

function getWheelZoomFactor(currentZoomFactor: number, event: ZoomWheelEvent, pageHeight: number) {
  const deltaModeMultiplier =
    event.deltaMode === 1 ? WHEEL_LINE_HEIGHT_PX : event.deltaMode === 2 ? pageHeight : 1;
  const normalizedDelta = Math.max(
    -MAX_WHEEL_DELTA_PX,
    Math.min(MAX_WHEEL_DELTA_PX, event.deltaY * deltaModeMultiplier)
  );

  return clampZoomFactor(currentZoomFactor * Math.exp(-normalizedDelta * WHEEL_ZOOM_SENSITIVITY));
}

function getAssetIntrinsicSize(selected: SelectedAsset | null): MediaDimensions {
  return {
    width: Math.max(0, selected?.width ?? 0),
    height: Math.max(0, selected?.height ?? 0)
  };
}

export function useLightboxImageControls({
  keyboardShortcutsEnabled = true,
  keyboardShortcutsSuspended = false,
  selected,
  onClose,
  onNavigatePrevious,
  onNavigateNext,
  onEnterFullscreen
}: UseLightboxImageControlsOptions) {
  const [isZoomed, setIsZoomed] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [mediaSize, setMediaSize] = useState<MediaDimensions>({ width: 0, height: 0 });
  const [intrinsicMediaSize, setIntrinsicMediaSize] = useState<MediaDimensions>(() => getAssetIntrinsicSize(selected));

  const lightboxShellRef = useRef<HTMLDivElement | null>(null);
  const mediaViewportRef = useRef<HTMLDivElement | null>(null);
  const lightboxImageRef = useRef<HTMLImageElement | null>(null);
  const lightboxVideoPlayerRef = useRef<LightboxVideoPlayerHandle | null>(null);
  const zoomFactorRef = useRef(MIN_ZOOM_FACTOR);
  const panRef = useRef({ x: 0, y: 0 });
  const renderFrameRef = useRef<number | null>(null);
  const handleImageWheelRef = useRef<(event: ZoomWheelEvent) => void>(() => {});
  const suppressImageClickUntilRef = useRef(0);
  const suppressCloseUntilRef = useRef(0);
  const dragRef = useRef({
    active: false,
    moved: false,
    pointerId: null as number | null,
    usesPointerCapture: false,
    startX: 0,
    startY: 0,
    originX: 0,
    originY: 0,
    captureTarget: null as HTMLElement | null
  });

  const selectedId = selected?.id ?? null;
  const selectedKind = selected?.kind ?? null;
  const isImageSelected = selectedKind !== null && selectedKind !== "video";
  const fittedMediaLayout = calculateFittedMediaSize(intrinsicMediaSize, mediaSize);
  const mediaDisplaySize = useMemo(
    () => ({
      width: fittedMediaLayout.width,
      height: fittedMediaLayout.height
    }),
    [fittedMediaLayout.height, fittedMediaLayout.width]
  );
  const fitScale = fittedMediaLayout.fitScale;

  const applyImageTransform = useCallback(() => {
    const image = lightboxImageRef.current;
    if (!image) return;

    // Grow the image's layout size so zoom paints detail from the original.
    image.style.setProperty("--lightbox-image-zoom", String(zoomFactorRef.current));
    image.style.transform = `translate(${panRef.current.x}px, ${panRef.current.y}px)`;
  }, []);

  const requestImageTransform = useCallback(() => {
    if (renderFrameRef.current !== null) return;

    renderFrameRef.current = window.requestAnimationFrame(() => {
      renderFrameRef.current = null;
      applyImageTransform();
    });
  }, [applyImageTransform]);

  const updateZoom = useCallback(
    (nextZoomFactor: number, anchor?: { x: number; y: number }) => {
      const image = lightboxImageRef.current;
      const viewport = mediaViewportRef.current;
      if (!image || !viewport) {
        return;
      }

      const resolvedMediaSize =
        mediaSize.width && mediaSize.height
          ? mediaSize
          : {
              width: viewport.clientWidth,
              height: viewport.clientHeight
            };
      const resolvedIntrinsicSize =
        intrinsicMediaSize.width && intrinsicMediaSize.height
          ? intrinsicMediaSize
          : {
              width: image.naturalWidth || Math.max(0, selected?.width ?? 0),
              height: image.naturalHeight || Math.max(0, selected?.height ?? 0)
            };
      const resolvedMediaDisplaySize =
        mediaDisplaySize.width && mediaDisplaySize.height
          ? mediaDisplaySize
          : calculateFittedMediaSize(resolvedIntrinsicSize, resolvedMediaSize);

      if (
        !resolvedMediaSize.width ||
        !resolvedMediaSize.height ||
        !resolvedMediaDisplaySize.width ||
        !resolvedMediaDisplaySize.height
      ) {
        return;
      }

      const clampedZoomFactor = clampZoomFactor(nextZoomFactor);
      const previousZoomFactor = zoomFactorRef.current;
      const rect = viewport.getBoundingClientRect();
      const anchorX = anchor?.x ?? rect.left + rect.width / 2;
      const anchorY = anchor?.y ?? rect.top + rect.height / 2;
      const viewportX = anchorX - (rect.left + rect.width / 2);
      const viewportY = anchorY - (rect.top + rect.height / 2);
      const sourceX = (viewportX - panRef.current.x) / previousZoomFactor;
      const sourceY = (viewportY - panRef.current.y) / previousZoomFactor;

      panRef.current = {
        x: viewportX - sourceX * clampedZoomFactor,
        y: viewportY - sourceY * clampedZoomFactor
      };
      zoomFactorRef.current = clampedZoomFactor;
      if ((previousZoomFactor > MIN_ZOOM_FACTOR) !== (clampedZoomFactor > MIN_ZOOM_FACTOR)) {
        setIsZoomed(clampedZoomFactor > MIN_ZOOM_FACTOR);
      }
      panRef.current = clampPan(panRef.current, clampedZoomFactor, resolvedMediaDisplaySize, resolvedMediaSize);
      requestImageTransform();
    },
    [intrinsicMediaSize, mediaDisplaySize, mediaSize, requestImageTransform, selected?.height, selected?.width]
  );

  const resetZoom = useCallback(() => {
    zoomFactorRef.current = MIN_ZOOM_FACTOR;
    setIsZoomed(false);
    panRef.current = { x: 0, y: 0 };
    requestImageTransform();
  }, [requestImageTransform]);

  const toggleFullscreen = useCallback(async () => {
    try {
      if (selectedKind === "video") {
        const player = lightboxVideoPlayerRef.current;
        if (!player) return;

        await player.toggleFullscreen();
        return;
      }

      const target = lightboxShellRef.current;
      if (!target) return;

      if (document.fullscreenElement === target) {
        await document.exitFullscreen();
        return;
      }

      if (document.fullscreenElement) {
        await document.exitFullscreen();
      }

      onEnterFullscreen();
      await target.requestFullscreen();
    } catch {
      // no-op
    }
  }, [onEnterFullscreen, selectedKind]);

  const suppressBackdropClose = useCallback((ms = 320) => {
    suppressCloseUntilRef.current = Date.now() + ms;
  }, []);

  const tryCloseLightbox = useCallback(() => {
    if (dragRef.current.active) return;
    if (Date.now() < suppressCloseUntilRef.current) return;
    onClose();
  }, [onClose]);

  const handleImageLoad = useCallback(
    (event: SyntheticEvent<HTMLImageElement>) => {
      const image = event.currentTarget;
      lightboxImageRef.current = image;
      setIntrinsicMediaSize({
        width: image.naturalWidth,
        height: image.naturalHeight
      });
      requestImageTransform();
    },
    [requestImageTransform]
  );

  const handleVideoLoadedMetadata = useCallback((dimensions: MediaDimensions) => {
    const nextSize = {
      width: dimensions.width || Math.max(0, selected?.width ?? 0),
      height: dimensions.height || Math.max(0, selected?.height ?? 0)
    };
    setIntrinsicMediaSize((current) =>
      current.width === nextSize.width && current.height === nextSize.height
        ? current
        : nextSize
    );
  }, [selected?.height, selected?.width]);

  const handleVideoFullscreenChange = useCallback((fullscreen: boolean) => {
    setIsFullscreen(fullscreen);
    if (fullscreen) {
      onEnterFullscreen();
    }
  }, [onEnterFullscreen]);

  const handleImageWheel = useCallback(
    (event: ZoomWheelEvent) => {
      if (!isImageSelected || event.deltaY === 0) {
        return;
      }

      event.preventDefault();
      suppressBackdropClose(360);
      const pageHeight = Math.max(1, mediaViewportRef.current?.clientHeight ?? 1);
      updateZoom(getWheelZoomFactor(zoomFactorRef.current, event, pageHeight), {
        x: event.clientX,
        y: event.clientY
      });
    },
    [isImageSelected, suppressBackdropClose, updateZoom]
  );

  useEffect(() => {
    handleImageWheelRef.current = handleImageWheel;
  }, [handleImageWheel]);

  useEffect(() => {
    if (!isImageSelected || selectedId === null) {
      return;
    }

    const viewport = mediaViewportRef.current;
    if (!viewport) {
      return;
    }

    const onWheel = (event: WheelEvent) => {
      handleImageWheelRef.current(event);
    };

    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [isImageSelected, selectedId]);

  const handleImageClick = useCallback(
    (event: ReactMouseEvent<HTMLImageElement>) => {
      if (!isImageSelected || Date.now() < suppressImageClickUntilRef.current) {
        return;
      }

      suppressBackdropClose(360);
      updateZoom(getNextZoomFactor(zoomFactorRef.current, "in"), {
        x: event.clientX,
        y: event.clientY
      });
    },
    [isImageSelected, suppressBackdropClose, updateZoom]
  );

  const beginImageDrag = useCallback(
    (clientX: number, clientY: number, usesPointerCapture: boolean) => {
      suppressBackdropClose(500);
      dragRef.current.active = true;
      dragRef.current.moved = false;
      dragRef.current.usesPointerCapture = usesPointerCapture;
      dragRef.current.startX = clientX;
      dragRef.current.startY = clientY;
      dragRef.current.originX = panRef.current.x;
      dragRef.current.originY = panRef.current.y;
      setIsDragging(true);
    },
    [suppressBackdropClose]
  );

  const updateImageDrag = useCallback(
    (clientX: number, clientY: number) => {
      if (!dragRef.current.active) return;

      const deltaX = clientX - dragRef.current.startX;
      const deltaY = clientY - dragRef.current.startY;
      if (
        Math.abs(deltaX) > DRAG_CLICK_SUPPRESSION_THRESHOLD_PX ||
        Math.abs(deltaY) > DRAG_CLICK_SUPPRESSION_THRESHOLD_PX
      ) {
        dragRef.current.moved = true;
      }

      panRef.current = clampPan(
        {
          x: dragRef.current.originX + deltaX,
          y: dragRef.current.originY + deltaY
        },
        zoomFactorRef.current,
        mediaDisplaySize,
        mediaSize
      );
      requestImageTransform();
    },
    [mediaDisplaySize, mediaSize, requestImageTransform]
  );

  const finishImageDrag = useCallback(() => {
    if (!dragRef.current.active) return;

    const capturedPointerId = dragRef.current.pointerId;
    const captureTarget = dragRef.current.captureTarget;
    if (dragRef.current.moved) {
      suppressImageClickUntilRef.current = Date.now() + 120;
    }
    dragRef.current.active = false;
    dragRef.current.moved = false;
    dragRef.current.pointerId = null;
    dragRef.current.usesPointerCapture = false;
    dragRef.current.captureTarget = null;
    setIsDragging(false);
    suppressBackdropClose(240);

    if (capturedPointerId !== null && captureTarget?.hasPointerCapture(capturedPointerId)) {
      try {
        captureTarget.releasePointerCapture(capturedPointerId);
      } catch {
        // Pointer capture can already be released by the browser during pointerup/cancel.
      }
    }
  }, [suppressBackdropClose]);

  const handleImagePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (
        !isImageSelected ||
        !event.isPrimary ||
        event.button !== 0 ||
        zoomFactorRef.current <= MIN_ZOOM_FACTOR
      ) {
        return;
      }

      event.preventDefault();
      let usesPointerCapture = false;
      try {
        event.currentTarget.setPointerCapture(event.pointerId);
        usesPointerCapture = event.currentTarget.hasPointerCapture(event.pointerId);
      } catch {
        // The window-level pointer listeners below keep drag working without capture.
      }
      dragRef.current.pointerId = event.pointerId;
      dragRef.current.captureTarget = usesPointerCapture ? event.currentTarget : null;
      beginImageDrag(event.clientX, event.clientY, usesPointerCapture);
    },
    [beginImageDrag, isImageSelected]
  );

  const handleImagePointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (
        !dragRef.current.active ||
        !dragRef.current.usesPointerCapture ||
        dragRef.current.pointerId !== event.pointerId
      ) {
        return;
      }

      event.preventDefault();
      updateImageDrag(event.clientX, event.clientY);
    },
    [updateImageDrag]
  );

  const handleImagePointerEnd = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (dragRef.current.pointerId !== event.pointerId) return;
      finishImageDrag();
    },
    [finishImageDrag]
  );

  const handleImageMouseDown = useCallback(
    (event: ReactMouseEvent<HTMLImageElement>) => {
      if (!isImageSelected || zoomFactorRef.current <= MIN_ZOOM_FACTOR) return;

      event.preventDefault();
      beginImageDrag(event.clientX, event.clientY, false);
    },
    [beginImageDrag, isImageSelected]
  );

  const handleImageDoubleClick = useCallback(
    (event?: ReactMouseEvent<HTMLImageElement>) => {
      if (!isImageSelected) {
        return;
      }

      suppressBackdropClose(420);

      if (zoomFactorRef.current > MIN_ZOOM_FACTOR) {
        resetZoom();
        return;
      }

      updateZoom(
        getDoubleClickZoomFactor(fitScale),
        event
          ? {
              x: event.clientX,
              y: event.clientY
            }
          : undefined
      );
    },
    [fitScale, isImageSelected, resetZoom, suppressBackdropClose, updateZoom]
  );

  useEffect(() => {
    if (selectedId === null) return;

    resetZoom();
    dragRef.current.active = false;
    dragRef.current.moved = false;
    dragRef.current.pointerId = null;
    dragRef.current.usesPointerCapture = false;
    dragRef.current.captureTarget = null;
    suppressImageClickUntilRef.current = 0;
    setIsDragging(false);
    setIntrinsicMediaSize(getAssetIntrinsicSize(selected));
  }, [resetZoom, selected?.height, selected?.id, selected?.width, selectedId]);

  useLightboxKeyboardShortcuts({
    enabled: keyboardShortcutsEnabled && (!keyboardShortcutsSuspended || (selectedKind === "video" && isFullscreen)),
    selected,
    isFullscreen,
    onNavigatePrevious,
    onNavigateNext,
    onToggleFullscreen: toggleFullscreen,
    onResetZoom: resetZoom,
    onZoomIn: () => {
      if (!isImageSelected) return;
      updateZoom(getNextZoomFactor(zoomFactorRef.current, "in"));
    },
    onZoomOut: () => {
      if (!isImageSelected) return;
      updateZoom(getNextZoomFactor(zoomFactorRef.current, "out"));
    }
  });

  useEffect(() => {
    const onFullscreenChange = () => {
      if (selectedKind === "video") {
        return;
      }

      const target = lightboxShellRef.current;
      setIsFullscreen(Boolean(target && document.fullscreenElement === target));
    };

    document.addEventListener("fullscreenchange", onFullscreenChange);
    onFullscreenChange();

    return () => {
      document.removeEventListener("fullscreenchange", onFullscreenChange);
    };
  }, [selectedId, selectedKind]);

  useEffect(() => {
    if (!isImageSelected || selectedId === null || !isDragging || !dragRef.current.active) {
      return;
    }

    const onBlur = () => {
      finishImageDrag();
    };

    window.addEventListener("blur", onBlur);

    if (dragRef.current.usesPointerCapture) {
      return () => window.removeEventListener("blur", onBlur);
    }

    if (dragRef.current.pointerId !== null) {
      const onPointerMove = (event: PointerEvent) => {
        if (dragRef.current.pointerId !== event.pointerId) return;
        event.preventDefault();
        updateImageDrag(event.clientX, event.clientY);
      };
      const onPointerEnd = (event: PointerEvent) => {
        if (dragRef.current.pointerId === event.pointerId) {
          finishImageDrag();
        }
      };

      window.addEventListener("pointermove", onPointerMove, { passive: false });
      window.addEventListener("pointerup", onPointerEnd);
      window.addEventListener("pointercancel", onPointerEnd);

      return () => {
        window.removeEventListener("blur", onBlur);
        window.removeEventListener("pointermove", onPointerMove);
        window.removeEventListener("pointerup", onPointerEnd);
        window.removeEventListener("pointercancel", onPointerEnd);
      };
    }

    const onMouseMove = (event: MouseEvent) => {
      updateImageDrag(event.clientX, event.clientY);
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", finishImageDrag);

    return () => {
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", finishImageDrag);
    };
  }, [finishImageDrag, isDragging, isImageSelected, selectedId, updateImageDrag]);

  const handleMediaSizeChange = useCallback((nextSize: MediaDimensions) => {
    setMediaSize((current) => {
      if (current.width === nextSize.width && current.height === nextSize.height) {
        return current;
      }

      return nextSize;
    });
  }, []);

  useLightboxViewportSize({
    selectedId,
    fullscreen: isFullscreen,
    mediaViewportRef,
    onSizeChange: handleMediaSizeChange
  });

  useEffect(() => {
    if (!isImageSelected || selectedId === null || zoomFactorRef.current <= MIN_ZOOM_FACTOR) return;

    panRef.current = clampPan(panRef.current, zoomFactorRef.current, mediaDisplaySize, mediaSize);
    requestImageTransform();
  }, [isImageSelected, mediaDisplaySize, mediaSize, requestImageTransform, selectedId]);

  useEffect(
    () => () => {
      if (renderFrameRef.current !== null) {
        window.cancelAnimationFrame(renderFrameRef.current);
        renderFrameRef.current = null;
      }
    },
    []
  );

  return {
    lightboxShellRef,
    mediaViewportRef,
    lightboxImageRef,
    lightboxVideoPlayerRef,
    get zoomLevel() {
      return zoomFactorRef.current;
    },
    fitScale,
    get zoomFactor() {
      return zoomFactorRef.current;
    },
    get renderScale() {
      return fitScale * zoomFactorRef.current;
    },
    isZoomed,
    mediaDisplaySize,
    isDragging,
    isFullscreen,
    resetZoom,
    toggleFullscreen,
    tryCloseLightbox,
    handleImageLoad,
    handleVideoLoadedMetadata,
    handleVideoFullscreenChange,
    handleImageWheel,
    handleImageClick,
    handleImagePointerDown,
    handleImagePointerMove,
    handleImagePointerEnd,
    handleImageMouseDown,
    handleImageDoubleClick
  };
}
