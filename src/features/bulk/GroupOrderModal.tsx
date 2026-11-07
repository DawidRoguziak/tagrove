import { createEffect, createMemo, createSignal, onCleanup, untrack, For } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import type { BulkController, BulkGroupSaveResult } from "./createBulk";
import type { ThumbnailStore } from "../../library/thumbnails/thumbnailStore";
import { createAssetThumbnail } from "../../library/thumbnails/createAssetThumbnail";
import { toMediaSrc } from "../../platform/tauri/assets";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../../ui/controls/ThumbnailImage";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiButton } from "../../ui/controls/UiButton";
import { UiIcon } from "../../ui/controls/UiIcon";
import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { UiModal } from "../../ui/overlays/UiModal";
import { createVirtualList } from "../../ui/dom/createVirtualList";
import { reorderAssetIdsByDrop } from "./bulkGroupOrderService";
function OrderThumbnail(props: { id: number; store: ThumbnailStore }) {
  const current = createAssetThumbnail(
    untrack(() => props.store),
    () => props.id
  );
  return (
    <>
      <ThumbnailImage
        src={current().path ? toMediaSrc(current().path!) : TRANSPARENT_THUMBNAIL_SRC}
        alt=""
        className="h-full w-full object-contain"
        draggable={false}
      />
      {current().rendering && !current().path ? (
        <span class="absolute inset-0 grid place-items-center bg-base-100/55" aria-hidden="true">
          <span class="h-4 w-4 animate-spin rounded-full border-2 border-base-content/25 border-t-primary" />
        </span>
      ) : null}
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

export function GroupOrderModal(props: {
  open: boolean;
  controller: BulkController;
  thumbnailStore: ThumbnailStore;
  onClose: () => void;
}) {
  const { t } = useLanguage();
  const open = createMemo(() => props.open);
  const [order, setOrder] = createSignal<number[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [result, setResult] = createSignal<BulkGroupSaveResult | null>(null);
  const [announcement, setAnnouncement] = createSignal("");
  const [draggedId, setDraggedId] = createSignal<number | null>(null);
  const [targetId, setTargetId] = createSignal<number | null>(null);
  const [width, setWidth] = createSignal(960);
  const [scroller, setScroller] = createSignal<HTMLDivElement>();
  let ghost: HTMLDivElement | undefined;
  let activeDrag: PointerDrag | null = null;
  let savePending = false;
  let session = 0;
  let mounted = true;
  let focusId: number | null = null;
  onCleanup(() => {
    mounted = false;
  });
  createEffect(open, (open) => {
    session++;
    if (!open) return;
    savePending = false;
    setSaving(false);
    setOrder([...props.controller.orderedAssetIds]);
    setResult(null);
    setAnnouncement("");
    setDraggedId(null);
    setTargetId(null);
  });
  const visible = createModalSnapshot(open, () => ({
    assets: props.controller.selectedAssets,
    key: props.controller.groupKeyDraft
  }));
  const assetsById = createMemo(() => new Map(visible().assets.map((asset) => [asset.id, asset])));
  const columns = () => Math.max(1, Math.floor((width() + 12) / 292));
  const tileWidth = () => Math.max(1, (width() - (columns() - 1) * 12) / columns());
  const previewHeight = () => Math.round(tileWidth() * 0.66);
  const rowHeight = () => previewHeight() + 56;
  const locked = () => saving() || props.controller.groupApplying;
  const list = createVirtualList({
    scroller,
    count: () => Math.ceil(order().length / columns()),
    estimateSize: rowHeight,
    overscan: 2,
    initialViewport: 560,
    enabled: open
  });
  createEffect(
    () => ({ open: open(), element: scroller() }),
    ({ open, element }) => {
      if (!open || !element) return;
      const resize = () => setWidth(element.clientWidth || 960);
      resize();
      const observer = new ResizeObserver(resize);
      observer.observe(element);
      return () => observer.disconnect();
    }
  );
  const layout = createModalSnapshot(open, () => ({ rows: list.items(), total: list.totalSize() }));
  const rows = () => layout().rows;
  const total = () => layout().total;
  createEffect(
    () => ({
      open: open(),
      ids: order().slice(
        (rows()[0]?.index ?? 0) * columns(),
        Math.min(order().length, ((rows()[rows().length - 1]?.index ?? -1) + 1) * columns())
      )
    }),
    (value) => {
      if (value.open) props.controller.queueThumbnailsByIds(value.ids);
    }
  );
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
  // Capture on the stable scroll container: virtual rows() can disappear during a drag.
  createEffect(open, (open) => {
    if (!open) return;
    let frame = 0;
    const dropTarget = (scroller: HTMLElement, x: number, y: number): number | null => {
      const viewport = scroller.getBoundingClientRect();
      if (x < viewport.left || x > viewport.right || y < viewport.top || y > viewport.bottom)
        return null;
      const direct = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-sort-asset]");
      if (direct && scroller.contains(direct)) return Number(direct.dataset.sortAsset);
      // Gaps between rows() and the space after the last tile are valid drop areas.
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
    const move = (event: PointerEvent) => {
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
      if (event.type === "pointerup" && drag.active && target !== null) {
        setOrder((previous) => reorderAssetIdsByDrop(previous, drag.id, target));
        focusId = drag.id;
      }
      activeDrag = null;
      if (scroller()?.hasPointerCapture(drag.pointerId))
        scroller()?.releasePointerCapture(drag.pointerId);
      cancelAnimationFrame(frame);
      frame = 0;
      setDraggedId(null);
      setTargetId(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    window.addEventListener("blur", finish);
    return () => {
      cancelAnimationFrame(frame);
      if (activeDrag && scroller()?.hasPointerCapture(activeDrag.pointerId))
        scroller()?.releasePointerCapture(activeDrag.pointerId);
      activeDrag = null;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      window.removeEventListener("blur", finish);
    };
  });

  const save = async () => {
    if (savePending || locked()) return;
    const capturedSession = session;
    savePending = true;
    setSaving(true);
    setResult(null);
    try {
      const outcome = await props.controller.onApplyGroup(order());
      if (!mounted || session !== capturedSession) return;
      setResult(outcome);
      if (outcome.status === "saved") props.onClose();
    } catch {
      if (mounted && session === capturedSession) setResult({ status: "failed" });
    } finally {
      if (mounted && session === capturedSession) {
        savePending = false;
        setSaving(false);
      }
    }
  };

  const draggedAsset = () =>
    draggedId() === null ? undefined : assetsById().get(draggedId() ?? -1);
  const draggedIndex = () => (draggedId() === null ? -1 : order().indexOf(draggedId() ?? -1));
  const targetIndex = () => (targetId() === null ? -1 : order().indexOf(targetId() ?? -1));
  const targetRow = () => rows().find((row) => row.index === Math.floor(targetIndex() / columns()));
  // The existing drop operation inserts after a later target and before an earlier one.
  const insertAfter = () => draggedIndex() < targetIndex();
  const showInsertionLine = () =>
    draggedIndex() >= 0 && targetIndex() >= 0 && draggedIndex() !== targetIndex();
  return (
    <UiModal
      open={open()}
      onClose={() => {
        if (!savePending && !locked()) props.onClose();
      }}
      size="xlarge"
      closeOnEscape={!locked()}
      closeOnOverlayClick={!locked()}
      ariaLabel={t("bulk.orderModal.title")}
      testId="bulk-order-modal"
      contentClassName="flex h-[90vh] min-h-0 flex-col gap-4 overflow-hidden"
    >
      <header class="shrink-0">
        <h2 class="m-0 text-lg font-semibold">{t("bulk.orderModal.title")}</h2>
        <p class="m-0 truncate text-sm text-[var(--text-muted)]" title={visible().key}>
          {visible().key} · {t("bulk.panel.selectedItems", { count: order().length })}
        </p>
        <p class="mt-1 text-xs text-[var(--text-muted)]">{t("bulk.orderModal.instructions")}</p>
      </header>
      <div
        ref={setScroller}
        class="panel-scroll relative min-h-0 flex-1 overflow-y-auto overscroll-contain"
        data-testid="bulk-order-grid"
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
                        data-testid={`bulk-order-tile-${id}`}
                        onPointerDown={(event) => {
                          if (event.button !== 0 || locked() || activeDrag) return;
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
                        class={`touch-none select-none overflow-hidden rounded-[var(--radius-control)] border border-base-content/15 bg-base-200 ${locked() ? "cursor-not-allowed" : "cursor-grab active:cursor-grabbing"} ${draggedId() === id ? "opacity-50" : ""}`}
                      >
                        <div class="relative" style={{ height: `${previewHeight()}px` }}>
                          <OrderThumbnail store={props.thumbnailStore} id={id} />
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
                            data-testid={`bulk-order-handle-${id}`}
                            class="grid h-8 w-8 shrink-0 touch-none select-none place-items-center rounded-[var(--radius-control)] hover:bg-base-content/10 disabled:opacity-50 cursor-grab active:cursor-grabbing"
                            disabled={locked()}
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
                              if (!delta || locked() || activeDrag) return;
                              event.preventDefault();
                              const nextIndex = index() + delta;
                              const target = order()[nextIndex];
                              if (target === undefined) return;
                              focusId = id;
                              setOrder((previous) => reorderAssetIdsByDrop(previous, id, target));
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
              data-testid="bulk-order-insertion-line"
              class="pointer-events-none absolute z-10 w-1 rounded-full bg-primary"
              style={{
                top: `${targetRow()!.start + 2}px`,
                height: `${rowHeight() - 16}px`,
                left: `${Math.max(
                  0,
                  Math.min(
                    width() - 4,
                    (targetIndex() % columns()) * (tileWidth() + 12) +
                      (insertAfter() ? tileWidth() + 4 : -8)
                  )
                )}px`
              }}
            />
          ) : null}
        </div>
      </div>
      {draggedAsset() ? (
        <div
          ref={(node) => {
            ghost = node;
          }}
          aria-hidden="true"
          class="pointer-events-none fixed left-0 top-0 z-[70] h-28 w-28 overflow-hidden rounded-[var(--radius-control)] border-2 border-primary bg-base-200 opacity-90 shadow-lg"
        >
          <OrderThumbnail store={props.thumbnailStore} id={draggedAsset()!.id} />
        </div>
      ) : null}
      <span class="sr-only" aria-live="polite">
        {announcement()}
      </span>
      <footer class="grid shrink-0 gap-3 border-t border-[var(--border-soft)] pt-4">
        {result()?.status === "failed" || result()?.status === "ignored" ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.groupSaveFailed")}
          </UiAlert>
        ) : result()?.status === "partial" ? (
          <UiAlert tone="warning" title={t("bulk.panel.partialResult", result() ?? {})}>
            {t("bulk.orderModal.partial")}
          </UiAlert>
        ) : null}
        <div class="flex justify-end gap-2">
          <UiButton disabled={locked()} onClick={props.onClose}>
            {t("common.cancel")}
          </UiButton>
          <UiButton
            variant="primary"
            disabled={
              locked() ||
              props.controller.selectionBusy ||
              props.controller.metadataLoading ||
              props.controller.metadataFailed
            }
            aria-busy={saving() ? "true" : "false"}
            onClick={() => void save()}
          >
            {saving() ? t("bulk.groupModal.applyInProgress") : t("bulk.orderModal.save")}
          </UiButton>
        </div>
      </footer>
    </UiModal>
  );
}
