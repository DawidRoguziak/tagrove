import { Portal } from "@solidjs/web";
import { createEffect, createMemo, createSignal, For } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import { thumbnailUrl } from "../../platform/tauri/thumbnails";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../../ui/controls/ThumbnailImage";
import { UiIcon } from "../../ui/controls/UiIcon";
import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { createVirtualList } from "../../ui/dom/createVirtualList";
import { reorderAssetIdsByDrop } from "./bulkGroupOrderService";

const GAP = 12;

function OrderThumbnail(props: { id: number; version: string | undefined }) {
  return (
    <>
      <ThumbnailImage
        src={
          props.version === undefined
            ? TRANSPARENT_THUMBNAIL_SRC
            : thumbnailUrl(props.id, props.version)
        }
        retrySrc={(retry) => thumbnailUrl(props.id, props.version ?? "", { retry })}
        alt=""
        className="h-full w-full object-contain"
        draggable={false}
      />
      <span
        class="thumbnail-loader absolute inset-0 grid place-items-center bg-base-100/55"
        aria-hidden="true"
      >
        <span class="h-4 w-4 animate-spin rounded-full border-2 border-base-content/25 border-t-primary" />
      </span>
    </>
  );
}

interface PointerDrag {
  id: number;
  pointerId: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  active: boolean;
  target: number | null;
}

export interface GroupOrderBoardProps {
  /** Enables the virtual list and the pointer listeners; inactive boards keep their last frame. */
  active: boolean;
  order: number[];
  onOrderChange: (next: number[]) => void;
  assets: readonly AssetSummary[];
  locked: boolean;
  /** Smallest tile width before the board drops a column. */
  minTileWidth: number;
  /** Prefix of the grid, tile, handle and insertion-line test IDs. */
  testIdPrefix: string;
  class?: string;
}

/**
 * Virtualized thumbnail grid that reorders a group by pointer drag or by arrow keys on each
 * tile's handle. The owner keeps the draft order; the board only reports the next order.
 */
export function GroupOrderBoard(props: GroupOrderBoardProps) {
  const { t } = useLanguage();
  const active = createMemo(() => props.active);
  // Thumbnail URLs need each asset's source-version token.
  const assetsById = createMemo(() => new Map(props.assets.map((asset) => [asset.id, asset])));
  const [announcement, setAnnouncement] = createSignal("");
  const [draggedId, setDraggedId] = createSignal<number | null>(null);
  const [targetId, setTargetId] = createSignal<number | null>(null);
  const [width, setWidth] = createSignal(960);
  const [scroller, setScroller] = createSignal<HTMLDivElement>();
  let ghost: HTMLDivElement | undefined;
  let activeDrag: PointerDrag | null = null;
  let focusId: number | null = null;
  const order = () => props.order;
  const columns = () => Math.max(1, Math.floor((width() + GAP) / (props.minTileWidth + GAP)));
  const tileWidth = () => Math.max(1, (width() - (columns() - 1) * GAP) / columns());
  const previewHeight = () => Math.round(tileWidth() * 0.66);
  const rowHeight = () => previewHeight() + 56;
  const list = createVirtualList({
    scroller,
    count: () => Math.ceil(order().length / columns()),
    estimateSize: rowHeight,
    overscan: 2,
    initialViewport: 560,
    enabled: active
  });
  createEffect(active, (isActive) => {
    if (!isActive) return;
    setAnnouncement("");
    setDraggedId(null);
    setTargetId(null);
  });
  createEffect(
    () => ({ active: active(), element: scroller() }),
    ({ active, element }) => {
      if (!active || !element) return;
      const resize = () => setWidth(element.clientWidth || 960);
      resize();
      const observer = new ResizeObserver(resize);
      observer.observe(element);
      return () => observer.disconnect();
    }
  );
  const layout = createModalSnapshot(active, () => ({
    rows: list.items(),
    total: list.totalSize()
  }));
  const rows = () => layout().rows;
  const total = () => layout().total;
  createEffect(
    () => ({ order: order(), rows: rows() }),
    () => {
      const id = focusId;
      if (id === null) return;
      const frame = requestAnimationFrame(() => {
        const handle = scroller()?.querySelector<HTMLButtonElement>(`[data-sort-handle="${id}"]`);
        if (!handle) return;
        handle.focus({ preventScroll: true });
        focusId = null;
        const asset = assetsById().get(id);
        if (asset)
          setAnnouncement(
            t("bulk.orderModal.moved", {
              name: asset.file_name,
              position: order().indexOf(id) + 1,
              count: order().length
            })
          );
      });
      return () => cancelAnimationFrame(frame);
    }
  );
  const move = (id: number, target: number) => {
    focusId = id;
    props.onOrderChange(reorderAssetIdsByDrop(order(), id, target));
  };
  // Capture on the stable scroll container: virtual rows can disappear during a drag.
  createEffect(active, (isActive) => {
    if (!isActive) return;
    let frame = 0;
    const dropTarget = (scroller: HTMLElement, x: number, y: number): number | null => {
      const viewport = scroller.getBoundingClientRect();
      if (x < viewport.left || x > viewport.right || y < viewport.top || y > viewport.bottom)
        return null;
      const direct = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-sort-asset]");
      if (direct && scroller.contains(direct)) return Number(direct.dataset.sortAsset);
      // Gaps between rows and the space after the last tile are valid drop areas.
      let nearest: number | null = null;
      let shortest = Number.POSITIVE_INFINITY;
      for (const tile of scroller.querySelectorAll<HTMLElement>("[data-sort-asset]")) {
        const rect = tile.getBoundingClientRect();
        const distance = Math.hypot(
          Math.max(rect.left - x, 0, x - rect.right),
          Math.max(rect.top - y, 0, y - rect.bottom)
        );
        if (distance < shortest) {
          shortest = distance;
          nearest = Number(tile.dataset.sortAsset);
        }
      }
      return nearest;
    };
    const tick = () => {
      const drag = activeDrag;
      const element = scroller();
      if (!drag || !element) {
        frame = 0;
        return;
      }
      if (drag.active) {
        const rect = element.getBoundingClientRect();
        if (
          drag.x >= rect.left &&
          drag.x <= rect.right &&
          drag.y >= rect.top &&
          drag.y <= rect.bottom
        ) {
          const edge = 56;
          const speed =
            drag.y < rect.top + edge
              ? -18 * (1 - (drag.y - rect.top) / edge)
              : drag.y > rect.bottom - edge
                ? 18 * (1 - (rect.bottom - drag.y) / edge)
                : 0;
          element.scrollTop += speed;
        }
        const nextTarget = dropTarget(element, drag.x, drag.y);
        if (nextTarget !== drag.target) {
          drag.target = nextTarget;
          setTargetId(nextTarget);
        }
        if (ghost) ghost.style.transform = `translate(${drag.x + 14}px, ${drag.y + 14}px)`;
      }
      frame = requestAnimationFrame(tick);
    };
    const pointerMove = (event: PointerEvent) => {
      const drag = activeDrag;
      if (!drag || event.pointerId !== drag.pointerId) return;
      drag.x = event.clientX;
      drag.y = event.clientY;
      if (!drag.active && Math.hypot(drag.x - drag.startX, drag.y - drag.startY) >= 5) {
        drag.active = true;
        setDraggedId(drag.id);
      }
      if (!frame) frame = requestAnimationFrame(tick);
    };
    const finish = (event: Event) => {
      const drag = activeDrag;
      if (!drag || (event instanceof PointerEvent && event.pointerId !== drag.pointerId)) return;
      if (event instanceof PointerEvent && event.type === "pointerup") {
        drag.x = event.clientX;
        drag.y = event.clientY;
        drag.active ||= Math.hypot(drag.x - drag.startX, drag.y - drag.startY) >= 5;
      }
      const element = scroller();
      const target = element ? dropTarget(element, drag.x, drag.y) : null;
      if (event.type === "pointerup" && drag.active && target !== null) move(drag.id, target);
      activeDrag = null;
      if (scroller()?.hasPointerCapture(drag.pointerId))
        scroller()?.releasePointerCapture(drag.pointerId);
      cancelAnimationFrame(frame);
      frame = 0;
      setDraggedId(null);
      setTargetId(null);
    };
    window.addEventListener("pointermove", pointerMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    window.addEventListener("blur", finish);
    return () => {
      cancelAnimationFrame(frame);
      if (activeDrag && scroller()?.hasPointerCapture(activeDrag.pointerId))
        scroller()?.releasePointerCapture(activeDrag.pointerId);
      activeDrag = null;
      setDraggedId(null);
      setTargetId(null);
      window.removeEventListener("pointermove", pointerMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      window.removeEventListener("blur", finish);
    };
  });

  const draggedAsset = () =>
    draggedId() === null ? undefined : assetsById().get(draggedId() ?? -1);
  const draggedIndex = () => (draggedId() === null ? -1 : order().indexOf(draggedId() ?? -1));
  const targetIndex = () => (targetId() === null ? -1 : order().indexOf(targetId() ?? -1));
  const targetRow = () => rows().find((row) => row.index === Math.floor(targetIndex() / columns()));
  // The drop operation inserts after a later target and before an earlier one.
  const insertAfter = () => draggedIndex() < targetIndex();
  const showInsertionLine = () =>
    draggedIndex() >= 0 && targetIndex() >= 0 && draggedIndex() !== targetIndex();
  return (
    <>
      <div
        ref={setScroller}
        class={`panel-scroll relative min-h-0 overflow-y-auto overscroll-contain ${props.class ?? ""}`}
        data-testid={`${props.testIdPrefix}-grid`}
        role="list"
        aria-label={t("bulk.panel.orderHeading")}
      >
        <div style={{ height: `${total()}px`, position: "relative" }}>
          <For each={rows()}>
            {(row) => (
              <div
                class="absolute left-0 top-0 grid w-full gap-3"
                style={{
                  transform: `translateY(${row.start}px)`,
                  "grid-template-columns": `repeat(${columns()}, minmax(0, 1fr))`
                }}
              >
                <For each={order().slice(row.index * columns(), (row.index + 1) * columns())}>
                  {(id, offset) => {
                    const asset = () => assetsById().get(id);
                    const index = () => row.index * columns() + offset();
                    return (
                      <div
                        role="listitem"
                        aria-posinset={index() + 1}
                        aria-setsize={order().length}
                        data-sort-asset={id}
                        data-testid={`${props.testIdPrefix}-tile-${id}`}
                        onPointerDown={(event) => {
                          if (event.button !== 0 || props.locked || activeDrag) return;
                          event.preventDefault();
                          event.currentTarget
                            .querySelector<HTMLButtonElement>("[data-sort-handle]")
                            ?.focus({ preventScroll: true });
                          activeDrag = {
                            id,
                            pointerId: event.pointerId,
                            startX: event.clientX,
                            startY: event.clientY,
                            x: event.clientX,
                            y: event.clientY,
                            active: false,
                            target: null
                          };
                          scroller()?.setPointerCapture(event.pointerId);
                        }}
                        class={`touch-none select-none overflow-hidden rounded-[var(--radius-control)] border border-base-content/15 bg-base-200 ${props.locked ? "cursor-not-allowed" : "cursor-grab active:cursor-grabbing"} ${draggedId() === id ? "opacity-50" : ""}`}
                      >
                        <div class="relative" style={{ height: `${previewHeight()}px` }}>
                          <OrderThumbnail id={id} version={asset()?.thumb_version} />
                        </div>
                        <div class="flex h-11 items-center gap-2 border-t border-[var(--border-soft)] px-3 font-mono">
                          <span class="text-xs font-semibold tabular-nums text-primary-text">
                            {index() + 1}
                          </span>
                          <span class="min-w-0 flex-1 truncate text-xs" title={asset()?.file_name}>
                            {asset()?.file_name}
                          </span>
                          <button
                            type="button"
                            data-sort-handle={id}
                            data-testid={`${props.testIdPrefix}-handle-${id}`}
                            class="grid h-8 w-8 shrink-0 touch-none select-none place-items-center rounded-[var(--radius-control)] hover:bg-base-content/10 disabled:opacity-50 cursor-grab active:cursor-grabbing"
                            disabled={props.locked}
                            aria-label={t("bulk.panel.dragHandleAria", { position: index() + 1 })}
                            aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown"
                            onKeyDown={(event) => {
                              const delta =
                                event.key === "ArrowLeft"
                                  ? -1
                                  : event.key === "ArrowRight"
                                    ? 1
                                    : event.key === "ArrowUp"
                                      ? -columns()
                                      : event.key === "ArrowDown"
                                        ? columns()
                                        : 0;
                              if (!delta || props.locked || activeDrag) return;
                              event.preventDefault();
                              const nextIndex = index() + delta;
                              const target = order()[nextIndex];
                              if (target === undefined) return;
                              move(id, target);
                              list.scrollToIndex(Math.floor(nextIndex / columns()));
                            }}
                          >
                            <UiIcon name="grip-vertical" className="h-5 w-5" />
                          </button>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            )}
          </For>
          {showInsertionLine() && targetRow() ? (
            <div
              aria-hidden="true"
              data-testid={`${props.testIdPrefix}-insertion-line`}
              class="pointer-events-none absolute z-10 w-1 rounded-full bg-primary"
              style={{
                top: `${targetRow()!.start + 2}px`,
                height: `${rowHeight() - 16}px`,
                left: `${Math.max(
                  0,
                  Math.min(
                    width() - 4,
                    (targetIndex() % columns()) * (tileWidth() + GAP) +
                      (insertAfter() ? tileWidth() + 4 : -8)
                  )
                )}px`
              }}
            />
          ) : null}
        </div>
      </div>
      {draggedAsset() ? (
        // Portaled so no ancestor stacking context (the dock's z-index) can paint over it.
        <Portal>
          <div
            ref={(node) => {
              ghost = node;
            }}
            aria-hidden="true"
            class="pointer-events-none fixed left-0 top-0 z-[70] h-28 w-28 overflow-hidden rounded-[var(--radius-control)] border-2 border-primary bg-base-200 opacity-90 shadow-lg"
          >
            <OrderThumbnail id={draggedAsset()!.id} version={draggedAsset()!.thumb_version} />
          </div>
        </Portal>
      ) : null}
      <span class="sr-only" aria-live="polite">
        {announcement()}
      </span>
    </>
  );
}
