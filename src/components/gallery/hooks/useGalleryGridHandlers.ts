import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
  type WheelEvent
} from "react";
import { useUiLayer } from "../../UI/UiLayerProvider";
import type { AssetSummary } from "../../../types";
import type { BulkSelectionHandler, SelectionRange } from "../selection";
import {
  containsIndex,
  edgeScrollSpeed,
  intersectedRanges,
  rectangleBetween,
  type GridGeometry,
  type Point,
  type SelectionRectangle
} from "../services/selectionGeometry";

interface Options extends GridGeometry {
  assets: AssetSummary[];
  getAssetAt?: (index: number) => AssetSummary | undefined;
  selectionModeEnabled: boolean;
  queryEpoch: number;
  gridOffset: number;
  gridRef: RefObject<HTMLDivElement | null>;
  scrollContainerRef?: RefObject<HTMLElement | null>;
  onCtrlWheelZoom?: (deltaY: number) => void;
  onSelect: (asset: AssetSummary) => void;
  onBulkSelectionInteraction?: BulkSelectionHandler;
}
interface Preview {
  rectangle: SelectionRectangle;
  ranges: SelectionRange[];
  additive: boolean;
}
interface Gesture {
  pointerId: number;
  pressedAt: number;
  element: HTMLElement;
  start: Point;
  origin: Point;
  latest: Point;
  additive: boolean;
  dragging: boolean;
  lastFrame: number;
}
const dragHoldDelayMs = 150;
const interactiveSelector =
  'button:not([data-asset-id]), input, select, textarea, a, [role="button"]:not([data-asset-id]), [contenteditable="true"]';
function isControl(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(target.closest(interactiveSelector));
}

export function useGalleryGridHandlers(options: Options) {
  const {
    selectionModeEnabled,
    queryEpoch,
    gridOffset,
    columnCount,
    tilePixelSize,
    tileGap,
    assetCount,
    gridRef,
    scrollContainerRef
  } = options;
  const current = useRef(options);
  current.current = options;
  const gesture = useRef<Gesture | null>(null);
  const frame = useRef<number | null>(null);
  const generation = useRef(0);
  const suppressClick = useRef(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [isDragSelecting, setIsDragSelecting] = useState(false);

  const release = useCallback(() => {
    const active = gesture.current;
    gesture.current = null;
    if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null;
    if (active?.element.hasPointerCapture(active.pointerId))
      active.element.releasePointerCapture(active.pointerId);
    setIsDragSelecting(false);
  }, []);
  const cancel = useCallback(() => {
    generation.current++;
    if (gesture.current?.dragging) suppressClick.current = true;
    release();
    setPreview(null);
    void current.current.onBulkSelectionInteraction?.({ type: "rectangle-cancel" });
  }, [release]);

  const clearSelection = useCallback(() => {
    if (gesture.current) suppressClick.current = true;
    cancel();
    void current.current.onBulkSelectionInteraction?.({ type: "clear" });
  }, [cancel]);

  useUiLayer({
    active: selectionModeEnabled,
    modal: false,
    containerRef: gridRef,
    onEscape: clearSelection
  });

  const updatePreview = useCallback((active: Gesture): Preview | null => {
    const grid = current.current.gridRef.current;
    const scroller = current.current.scrollContainerRef?.current ?? active.element;
    if (!grid) return null;
    const bounds = grid.getBoundingClientRect();
    const viewport = scroller.getBoundingClientRect();
    const x = Math.max(
      viewport.left + scroller.clientLeft,
      Math.min(active.latest.x, viewport.left + scroller.clientLeft + scroller.clientWidth)
    );
    const y = Math.max(
      viewport.top + scroller.clientTop,
      Math.min(active.latest.y, viewport.top + scroller.clientTop + scroller.clientHeight)
    );
    const rect = rectangleBetween(active.start, { x: x - bounds.left, y: y - bounds.top });
    const gallery = active.element.getBoundingClientRect();
    const left = Math.max(bounds.left + rect.left, viewport.left, gallery.left);
    const top = Math.max(bounds.top + rect.top, viewport.top, gallery.top);
    const right = Math.min(
      bounds.left + rect.left + rect.width,
      viewport.left + scroller.clientWidth,
      gallery.right
    );
    const bottom = Math.min(
      bounds.top + rect.top + rect.height,
      viewport.top + scroller.clientHeight,
      gallery.bottom
    );
    const next = {
      additive: active.additive,
      ranges: intersectedRanges(rect, current.current),
      rectangle: { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) }
    };
    setPreview(next);
    return next;
  }, []);

  const animate = useCallback(
    function tick(time: number) {
      const active = gesture.current;
      if (!active?.dragging) return;
      const scroller = current.current.scrollContainerRef?.current ?? active.element;
      const bounds = scroller.getBoundingClientRect();
      const elapsed = Math.min(32, time - active.lastFrame);
      active.lastFrame = time;
      scroller.scrollTop +=
        (edgeScrollSpeed(active.latest.y, bounds.top, bounds.top + scroller.clientHeight) *
          elapsed) /
        1000;
      updatePreview(active);
      frame.current = requestAnimationFrame(tick);
    },
    [updatePreview]
  );

  const handlePointerDown = useCallback(
    (event: PointerEvent<HTMLElement>) => {
      if (event.button !== 0 || !event.isPrimary) return;
      // A new press must not consume click suppression left by an earlier drag.
      suppressClick.current = false;
      if (!current.current.selectionModeEnabled || isControl(event.target)) return;
      const grid = current.current.gridRef.current;
      if (!grid) return;
      const scroller = current.current.scrollContainerRef?.current ?? event.currentTarget;
      const viewport = scroller.getBoundingClientRect();
      if (
        event.clientX >= viewport.left + scroller.clientLeft + scroller.clientWidth ||
        event.clientY >= viewport.top + scroller.clientTop + scroller.clientHeight
      )
        return;
      cancel();
      suppressClick.current = false;
      void current.current.onBulkSelectionInteraction?.({ type: "rectangle-start" });
      const bounds = grid.getBoundingClientRect();
      gesture.current = {
        pointerId: event.pointerId,
        pressedAt: performance.now(),
        element: event.currentTarget,
        start: { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
        origin: { x: event.clientX, y: event.clientY },
        latest: { x: event.clientX, y: event.clientY },
        additive: event.ctrlKey || event.metaKey,
        dragging: false,
        lastFrame: 0
      };
    },
    [cancel]
  );

  const handlePointerMove = useCallback(
    (
      event: Pick<PointerEvent<HTMLElement>, "pointerId" | "clientX" | "clientY" | "preventDefault">
    ) => {
      const active = gesture.current;
      if (!active || active.pointerId !== event.pointerId) return;
      active.latest = { x: event.clientX, y: event.clientY };
      if (
        !active.dragging &&
        performance.now() - active.pressedAt >= dragHoldDelayMs &&
        Math.hypot(event.clientX - active.origin.x, event.clientY - active.origin.y) >= 5
      ) {
        active.dragging = true;
        active.element.setPointerCapture(active.pointerId);
        active.lastFrame = performance.now();
        suppressClick.current = true;
        setIsDragSelecting(true);
        updatePreview(active);
        frame.current = requestAnimationFrame(animate);
      }
      if (active.dragging) event.preventDefault();
    },
    [animate, updatePreview]
  );

  const handlePointerUp = useCallback(
    (
      event: Pick<PointerEvent<HTMLElement>, "pointerId" | "clientX" | "clientY" | "preventDefault">
    ) => {
      const active = gesture.current;
      if (!active || active.pointerId !== event.pointerId) return;
      if (!active.dragging) {
        cancel();
        return;
      }
      active.latest = { x: event.clientX, y: event.clientY };
      const final = updatePreview(active);
      const request = generation.current;
      release();
      if (final) {
        void Promise.resolve(
          current.current.onBulkSelectionInteraction?.({
            type: "rectangle-commit",
            ranges: final.ranges,
            additive: final.additive
          })
        ).finally(() => {
          if (generation.current === request) setPreview(null);
        });
      }
    },
    [cancel, release, updatePreview]
  );

  useEffect(() => {
    window.addEventListener("blur", cancel);
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("resize", cancel);
    const sizes = new Map<Element, string>();
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const size = `${entry.contentRect.width},${entry.contentRect.height}`;
        const previous = sizes.get(entry.target);
        sizes.set(entry.target, size);
        if (previous !== undefined && previous !== size) cancel();
      }
    });
    if (gridRef.current) observer.observe(gridRef.current);
    if (scrollContainerRef?.current) observer.observe(scrollContainerRef.current);
    return () => {
      window.removeEventListener("blur", cancel);
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("resize", cancel);
      observer.disconnect();
      cancel();
    };
  }, [
    cancel,
    handlePointerMove,
    handlePointerUp,
    selectionModeEnabled,
    queryEpoch,
    gridOffset,
    columnCount,
    tilePixelSize,
    tileGap,
    assetCount,
    gridRef,
    scrollContainerRef
  ]);

  const handleTileClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const { assets, getAssetAt, onSelect, selectionModeEnabled, onBulkSelectionInteraction } =
        current.current;
      const assetId = Number(event.currentTarget.dataset.assetId);
      const assetIndex = Number(event.currentTarget.dataset.assetIndex);
      if (!Number.isInteger(assetId) || !Number.isInteger(assetIndex) || assetIndex < 0) return;
      const asset = getAssetAt?.(assetIndex) ?? assets.find((item) => item.id === assetId);
      if (!asset || asset.id !== assetId) return;
      if (!selectionModeEnabled) {
        onSelect(asset);
        return;
      }
      generation.current++;
      release();
      setPreview(null);
      event.preventDefault();
      void onBulkSelectionInteraction?.({
        type: "click",
        assetId,
        assetIndex,
        ctrlLike: event.ctrlKey || event.metaKey,
        shift: event.shiftKey
      });
    },
    [release]
  );

  const handleClickCapture = useCallback((event: MouseEvent<HTMLElement>) => {
    if (suppressClick.current && event.detail !== 0) {
      suppressClick.current = false;
      event.preventDefault();
      event.stopPropagation();
    }
  }, []);
  const handleGalleryWheel = useCallback((event: WheelEvent<HTMLElement>) => {
    if (!event.ctrlKey) return;
    event.preventDefault();
    current.current.onCtrlWheelZoom?.(event.deltaY);
  }, []);

  return {
    isDragSelecting,
    preview,
    handleGalleryWheel,
    handleTileClick,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel: cancel,
    handleLostPointerCapture: () => {
      if (gesture.current) cancel();
    },
    handleClickCapture,
    isPreviewSelected: (index: number, selected: boolean) =>
      preview ? containsIndex(preview.ranges, index) || (preview.additive && selected) : selected
  };
}
