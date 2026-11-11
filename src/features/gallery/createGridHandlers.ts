import { createEffect, createMemo, createSignal, untrack } from "solid-js";
import { createUiLayer } from "../../ui/overlays/UiLayerProvider";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import type { BulkSelectionHandler, SelectionRange } from "./selection";
import {
  containsIndex,
  edgeScrollSpeed,
  intersectedRanges,
  rectangleBetween,
  type GridGeometry,
  type Point,
  type SelectionRectangle
} from "./selectionGeometry";
interface Options extends GridGeometry {
  assets: AssetSummary[];
  getAssetAt?: (index: number) => AssetSummary | undefined;
  selectionModeEnabled: boolean;
  queryEpoch: number;
  gridOffset: number;
  scrollShift?: number;
  scrollBy?: (delta: number) => void;
  gridRef: () => HTMLDivElement | undefined;
  scrollContainerRef?: () => HTMLElement | undefined;
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
const editableSelector = 'input, textarea, select, [contenteditable="true"]';
const interactiveSelector =
  'button:not([data-asset-id]), input, select, textarea, a, [role="button"]:not([data-asset-id]), [contenteditable="true"]';
function isControl(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(target.closest(interactiveSelector));
}
export function createGridHandlers(options: Options) {
  let gesture: Gesture | null = null;
  let frame: number | null = null;
  let generation = 0;
  let suppressClick = false;
  const [preview, setPreview] = createSignal<Preview | null>(null);
  const [isDragSelecting, setIsDragSelecting] = createSignal(false);
  const release = () => {
    const active = gesture;
    gesture = null;
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    if (active?.element.hasPointerCapture(active.pointerId))
      active.element.releasePointerCapture(active.pointerId);
    setIsDragSelecting(false);
  };
  const cancel = () => {
    generation++;
    if (gesture?.dragging) suppressClick = true;
    release();
    setPreview(null);
    void options.onBulkSelectionInteraction?.({ type: "rectangle-cancel" });
  };
  const cancelForLayout = () => {
    if (gesture) cancel();
    else {
      // Released ranges use global indices. Layout only invalidates their preview,
      // not a pending read or the retry intent retained after a failed read.
      generation++;
      setPreview(null);
    }
  };
  const clearSelection = () => {
    if (gesture) suppressClick = true;
    cancel();
    void options.onBulkSelectionInteraction?.({ type: "clear" });
  };
  const layer = createUiLayer({
    active: () => options.selectionModeEnabled,
    modal: false,
    container: options.gridRef,
    onEscape: clearSelection
  });
  // Ctrl+A selects every result of the query without reading a single ID. Text fields
  // and any overlay above the gallery keep their own Ctrl+A.
  createEffect(
    () => options.selectionModeEnabled,
    (enabled) => {
      if (!enabled) return;
      const keydown = (event: KeyboardEvent) => {
        if (event.defaultPrevented || event.altKey || event.shiftKey) return;
        if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "a") return;
        const target = event.target;
        if (target instanceof Element && target.closest(editableSelector)) return;
        if (!layer.isTopLayer()) return;
        event.preventDefault();
        cancel();
        void options.onBulkSelectionInteraction?.({ type: "select-all" });
      };
      window.addEventListener("keydown", keydown);
      return () => window.removeEventListener("keydown", keydown);
    }
  );
  const updatePreview = (active: Gesture): Preview | null => {
    const grid = options.gridRef();
    const scroller = options.scrollContainerRef?.() ?? active.element;
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
    const shift = options.scrollShift ?? 0;
    const rect = rectangleBetween(active.start, { x: x - bounds.left, y: y - bounds.top + shift });
    const gallery = active.element.getBoundingClientRect();
    const left = Math.max(bounds.left + rect.left, viewport.left, gallery.left);
    const top = Math.max(bounds.top + rect.top - shift, viewport.top, gallery.top);
    const right = Math.min(
      bounds.left + rect.left + rect.width,
      viewport.left + scroller.clientWidth,
      gallery.right
    );
    const bottom = Math.min(
      bounds.top + rect.top - shift + rect.height,
      viewport.top + scroller.clientHeight,
      gallery.bottom
    );
    const next = {
      additive: active.additive,
      ranges: intersectedRanges(rect, options),
      rectangle: { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) }
    };
    setPreview(next);
    return next;
  };
  const animate = function tick(time: number) {
    const active = gesture;
    if (!active?.dragging) return;
    const scroller = options.scrollContainerRef?.() ?? active.element;
    const bounds = scroller.getBoundingClientRect();
    const elapsed = Math.min(32, time - active.lastFrame);
    active.lastFrame = time;
    const delta =
      (edgeScrollSpeed(active.latest.y, bounds.top, bounds.top + scroller.clientHeight) * elapsed) /
      1000;
    if (options.scrollBy) options.scrollBy(delta);
    else scroller.scrollTop += delta;
    updatePreview(active);
    frame = requestAnimationFrame(tick);
  };
  const handlePointerDown = (event: PointerEvent & { currentTarget: HTMLElement }) => {
    if (event.button !== 0 || !event.isPrimary) return;
    // A new press must not consume click suppression left by an earlier drag.
    suppressClick = false;
    if (!options.selectionModeEnabled || isControl(event.target)) return;
    const grid = options.gridRef();
    if (!grid) return;
    const scroller = options.scrollContainerRef?.() ?? event.currentTarget;
    const viewport = scroller.getBoundingClientRect();
    if (
      event.clientX >= viewport.left + scroller.clientLeft + scroller.clientWidth ||
      event.clientY >= viewport.top + scroller.clientTop + scroller.clientHeight
    )
      return;
    cancel();
    suppressClick = false;
    void options.onBulkSelectionInteraction?.({ type: "rectangle-start" });
    const bounds = grid.getBoundingClientRect();
    gesture = {
      pointerId: event.pointerId,
      pressedAt: performance.now(),
      element: event.currentTarget,
      start: { x: event.clientX - bounds.left, y: event.clientY - bounds.top + (options.scrollShift ?? 0) },
      origin: { x: event.clientX, y: event.clientY },
      latest: { x: event.clientX, y: event.clientY },
      additive: event.ctrlKey || event.metaKey,
      dragging: false,
      lastFrame: 0
    };
  };
  const handlePointerMove = (
    event: Pick<
      PointerEvent & { currentTarget: HTMLElement },
      "pointerId" | "clientX" | "clientY" | "preventDefault"
    >
  ) => {
    const active = gesture;
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
      suppressClick = true;
      setIsDragSelecting(true);
      updatePreview(active);
      frame = requestAnimationFrame(animate);
    }
    if (active.dragging) event.preventDefault();
  };
  const handlePointerUp = (
    event: Pick<
      PointerEvent & { currentTarget: HTMLElement },
      "pointerId" | "clientX" | "clientY" | "preventDefault"
    >
  ) => {
    const active = gesture;
    if (!active || active.pointerId !== event.pointerId) return;
    if (!active.dragging) {
      cancel();
      return;
    }
    active.latest = { x: event.clientX, y: event.clientY };
    const final = updatePreview(active);
    const request = generation;
    release();
    if (final) {
      void Promise.resolve(
        options.onBulkSelectionInteraction?.({
          type: "rectangle-commit",
          ranges: final.ranges,
          additive: final.additive
        })
      ).finally(() => {
        if (generation === request) setPreview(null);
      });
    }
  };
  // Geometry-only prop updates must not retire selection work.
  const selectionMode = createMemo(() => options.selectionModeEnabled);
  const queryEpoch = createMemo(() => options.queryEpoch);
  createEffect(selectionMode, () => () => untrack(cancel));
  createEffect(queryEpoch, () => () => untrack(cancel));
  createEffect(
    () => [
      options.gridOffset,
      options.columnCount,
      options.tilePixelSize,
      options.tileGap,
      options.assetCount,
      options.gridRef(),
      options.scrollContainerRef?.()
    ],
    () => {
      window.addEventListener("blur", cancel);
      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerUp);
      window.addEventListener("resize", cancelForLayout);
      return () => {
        window.removeEventListener("blur", cancel);
        window.removeEventListener("pointermove", handlePointerMove);
        window.removeEventListener("pointerup", handlePointerUp);
        window.removeEventListener("resize", cancelForLayout);
        cancelForLayout();
      };
    }
  );
  // The observer follows the elements only. The virtual grid publishes geometry from its
  // own ResizeObserver callback; an observer created in that delivery would leave its
  // first notification undelivered ("ResizeObserver loop completed ...").
  createEffect(
    () => ({ grid: options.gridRef(), scroller: options.scrollContainerRef?.() }),
    ({ grid, scroller }) => {
      const sizes = new Map<Element, string>();
      const observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const size = `${entry.contentRect.width},${entry.contentRect.height}`;
          const previous = sizes.get(entry.target);
          sizes.set(entry.target, size);
          if (previous !== undefined && previous !== size) cancelForLayout();
        }
      });
      if (grid) observer.observe(grid);
      if (scroller) observer.observe(scroller);
      return () => observer.disconnect();
    }
  );
  const handleTileClick = (event: MouseEvent & { currentTarget: HTMLButtonElement }) => {
    const { assets, getAssetAt, onSelect, selectionModeEnabled, onBulkSelectionInteraction } =
      options;
    const assetId = Number(event.currentTarget.dataset.assetId);
    const assetIndex = Number(event.currentTarget.dataset.assetIndex);
    if (!Number.isInteger(assetId) || !Number.isInteger(assetIndex) || assetIndex < 0) return;
    const asset = getAssetAt?.(assetIndex) ?? assets.find((item) => item.id === assetId);
    if (!asset || asset.id !== assetId) return;
    if (!selectionModeEnabled) {
      onSelect(asset);
      return;
    }
    generation++;
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
  };
  const handleClickCapture = (event: MouseEvent) => {
    if (suppressClick && event.detail !== 0) {
      suppressClick = false;
      event.preventDefault();
      event.stopPropagation();
    }
  };
  const handleGalleryWheel = (event: WheelEvent & { currentTarget: HTMLElement }) => {
    if (!event.ctrlKey) return;
    event.preventDefault();
    options.onCtrlWheelZoom?.(event.deltaY);
  };
  return {
    get isDragSelecting() {
      return isDragSelecting();
    },
    get preview() {
      return preview();
    },
    handleGalleryWheel,
    handleTileClick,
    handlePointerDown,
    handlePointerMove,
    handlePointerUp,
    handlePointerCancel: cancel,
    handleLostPointerCapture: () => {
      if (gesture) cancel();
    },
    handleClickCapture,
    isPreviewSelected: (index: number, selected: boolean) => {
      const value = preview();
      return value ? containsIndex(value.ranges, index) || (value.additive && selected) : selected;
    }
  };
}
