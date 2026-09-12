import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useTranslation } from "react-i18next";
import type { BulkGroupSaveResult, BulkSelectionController } from "../app/types";
import { ThumbnailSubscription } from "../UI/ThumbnailSubscription";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
import { UiIcon } from "../UI/UiIcon";
import { UiModal } from "../UI/UiModal";
import { reorderAssetIdsByDrop } from "./grouping/services/bulkGroupOrderService";

interface GroupOrderModalProps {
  open?: boolean;
  controller: BulkSelectionController;
  thumbs: Record<number, string>;
  renderingThumbnailIds: Record<number, true>;
  onClose: () => void;
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

export function GroupOrderModal({
  open = true,
  controller,
  thumbs,
  renderingThumbnailIds,
  onClose
}: GroupOrderModalProps) {
  const { t } = useTranslation();
  const [order, setOrder] = useState(() => [...controller.orderedAssetIds]);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<BulkGroupSaveResult | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [draggedId, setDraggedId] = useState<number | null>(null);
  const [targetId, setTargetId] = useState<number | null>(null);
  const savingRef = useRef(false);
  const sessionRef = useRef(0);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    sessionRef.current += 1;
    if (open) {
      savingRef.current = false;
      setSaving(false);
      setOrder([...controller.orderedAssetIds]);
      setResult(null);
      setAnnouncement("");
      setDraggedId(null);
      setTargetId(null);
    }
  }
  const [width, setWidth] = useState(960);
  const scrollRef = useRef<HTMLDivElement>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<PointerDrag | null>(null);
  const mountedRef = useRef(true);
  const focusIdRef = useRef<number | null>(null);
  const assetsById = useMemo(
    () => new Map(controller.selectedAssets.map((asset) => [asset.id, asset])),
    [controller.selectedAssets]
  );
  const columns = Math.max(1, Math.floor((width + 12) / 232));
  const tileWidth = Math.max(1, (width - (columns - 1) * 12) / columns);
  const rowHeight = tileWidth + 48;
  const virtualizer = useVirtualizer({
    count: Math.ceil(order.length / columns),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 2,
    initialRect: { width: 960, height: 560 }
  });
  const rows = virtualizer.getVirtualItems();
  const firstIndex = (rows[0]?.index ?? 0) * columns;
  const lastIndex = Math.min(order.length, ((rows.at(-1)?.index ?? -1) + 1) * columns);
  const locked = saving || controller.groupApplying;

  useEffect(() => {
    if (!open) return;
    const element = scrollRef.current;
    if (!element) return;
    const resize = () => setWidth(element.clientWidth || 960);
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element);
    return () => observer.disconnect();
  }, [open]);

  useEffect(() => {
    virtualizer.measure();
  }, [virtualizer, rowHeight]);

  useEffect(() => {
    if (!open) return;
    controller.queueThumbnailsByIds?.(order.slice(firstIndex, lastIndex));
  }, [open, controller.queueThumbnailsByIds, order, firstIndex, lastIndex]);

  useEffect(() => {
    const id = focusIdRef.current;
    if (id === null) return;
    const handle = scrollRef.current?.querySelector<HTMLButtonElement>(
      `[data-sort-handle="${id}"]`
    );
    if (handle) {
      handle.focus({ preventScroll: true });
      focusIdRef.current = null;
      const asset = assetsById.get(id);
      if (asset)
        setAnnouncement(
          t("bulk.orderModal.moved", {
            name: asset.file_name,
            position: order.indexOf(id) + 1,
            count: order.length
          })
        );
    }
  });

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Capture on the stable scroll container: virtual rows can disappear during a drag.
  useEffect(() => {
    if (!open) return;
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
      const drag = dragRef.current;
      const scroller = scrollRef.current;
      if (!drag || !scroller) {
        frame = 0;
        return;
      }
      if (drag.active) {
        const rect = scroller.getBoundingClientRect();
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
          scroller.scrollTop += speed;
        }
        const nextTarget = dropTarget(scroller, drag.x, drag.y);
        if (nextTarget !== drag.target) {
          drag.target = nextTarget;
          setTargetId(nextTarget);
        }
        if (ghostRef.current)
          ghostRef.current.style.transform = `translate(${drag.x + 14}px, ${drag.y + 14}px)`;
      }
      frame = requestAnimationFrame(tick);
    };
    const move = (event: PointerEvent) => {
      const drag = dragRef.current;
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
      const drag = dragRef.current;
      if (!drag || (event instanceof PointerEvent && event.pointerId !== drag.pointerId)) return;
      if (event instanceof PointerEvent && event.type === "pointerup") {
        drag.x = event.clientX;
        drag.y = event.clientY;
        drag.active ||= Math.hypot(drag.x - drag.startX, drag.y - drag.startY) >= 5;
      }
      const target = scrollRef.current ? dropTarget(scrollRef.current, drag.x, drag.y) : null;
      if (event.type === "pointerup" && drag.active && target !== null) {
        setOrder((previous) => reorderAssetIdsByDrop(previous, drag.id, target));
        focusIdRef.current = drag.id;
      }
      dragRef.current = null;
      if (scrollRef.current?.hasPointerCapture(drag.pointerId))
        scrollRef.current.releasePointerCapture(drag.pointerId);
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
      dragRef.current = null;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      window.removeEventListener("blur", finish);
    };
  }, [open]);

  const save = async () => {
    if (savingRef.current || locked) return;
    const session = sessionRef.current;
    savingRef.current = true;
    setSaving(true);
    setResult(null);
    try {
      const outcome = await controller.onApplyGroup(order);
      if (!mountedRef.current || sessionRef.current !== session) return;
      setResult(outcome);
      if (outcome.status === "saved") onClose();
    } catch {
      if (mountedRef.current && sessionRef.current === session) setResult({ status: "failed" });
    } finally {
      if (mountedRef.current && sessionRef.current === session) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  };

  const draggedAsset = draggedId === null ? undefined : assetsById.get(draggedId);
  const draggedIndex = draggedId === null ? -1 : order.indexOf(draggedId);
  const targetIndex = targetId === null ? -1 : order.indexOf(targetId);
  const targetRow = rows.find((row) => row.index === Math.floor(targetIndex / columns));
  // The existing drop operation inserts after a later target and before an earlier one.
  const insertAfter = draggedIndex < targetIndex;
  const showInsertionLine = draggedIndex >= 0 && targetIndex >= 0 && draggedIndex !== targetIndex;
  return (
    <UiModal
      open={open}
      onClose={() => {
        if (!savingRef.current && !locked) onClose();
      }}
      size="xlarge"
      closeOnEscape={!locked}
      closeOnOverlayClick={!locked}
      ariaLabel={t("bulk.orderModal.title")}
      testId="bulk-order-modal"
      contentClassName="flex h-[90vh] min-h-0 flex-col gap-4 overflow-hidden"
    >
      <header className="shrink-0">
        <h2 className="m-0 text-lg font-semibold">{t("bulk.orderModal.title")}</h2>
        <p className="m-0 truncate text-sm text-[var(--text-muted)]" title={controller.groupKeyDraft}>
          {controller.groupKeyDraft} · {t("bulk.panel.selectedItems", { count: order.length })}
        </p>
        <p className="mt-1 text-xs text-[var(--text-muted)]">{t("bulk.orderModal.instructions")}</p>
      </header>
      <div
        ref={scrollRef}
        className="panel-scroll relative min-h-0 flex-1 overflow-y-auto overscroll-contain"
        data-testid="bulk-order-grid"
        role="list"
        aria-label={t("bulk.panel.orderHeading")}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {rows.map((row) => (
            <div
              key={row.key}
              className="absolute left-0 top-0 grid w-full gap-3"
              style={{
                transform: `translateY(${row.start}px)`,
                gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`
              }}
            >
              {order.slice(row.index * columns, (row.index + 1) * columns).map((id, offset) => {
                const asset = assetsById.get(id);
                if (!asset) return null;
                const index = row.index * columns + offset;
                return (
                  <div
                    key={id}
                    role="listitem"
                    aria-posinset={index + 1}
                    aria-setsize={order.length}
                    data-sort-asset={id}
                    data-testid={`bulk-order-tile-${id}`}
                    onPointerDown={(event) => {
                      if (event.button !== 0 || locked || dragRef.current) return;
                      event.preventDefault();
                      event.currentTarget
                        .querySelector<HTMLButtonElement>("[data-sort-handle]")
                        ?.focus({ preventScroll: true });
                      dragRef.current = {
                        id,
                        pointerId: event.pointerId,
                        startX: event.clientX,
                        startY: event.clientY,
                        x: event.clientX,
                        y: event.clientY,
                        active: false,
                        target: null
                      };
                      scrollRef.current?.setPointerCapture(event.pointerId);
                    }}
                    className={`touch-none select-none overflow-hidden rounded-[var(--radius-control)] border border-base-content/15 bg-base-200 ${locked ? "cursor-not-allowed" : "cursor-grab active:cursor-grabbing"} ${draggedId === id ? "opacity-50" : ""}`}
                  >
                    <div className="relative" style={{ height: tileWidth - 2 }}>
                      <ThumbnailSubscription
                        id={id}
                        path={thumbs[id]}
                        rendering={Boolean(renderingThumbnailIds[id])}
                        fit="contain"
                      />
                    </div>
                    <div className="flex h-9 items-center gap-2 px-2">
                      <span className="text-xs font-semibold tabular-nums text-primary">
                        {index + 1}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-xs" title={asset.file_name}>
                        {asset.file_name}
                      </span>
                      <button
                        type="button"
                        data-sort-handle={id}
                        data-testid={`bulk-order-handle-${id}`}
                        className="grid h-8 w-8 shrink-0 touch-none select-none place-items-center rounded-[var(--radius-control)] hover:bg-base-content/10 disabled:opacity-50 cursor-grab active:cursor-grabbing"
                        disabled={locked}
                        aria-label={t("bulk.panel.dragHandleAria", { position: index + 1 })}
                        aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown"
                        onKeyDown={(event) => {
                          const delta =
                            event.key === "ArrowLeft"
                              ? -1
                              : event.key === "ArrowRight"
                                ? 1
                                : event.key === "ArrowUp"
                                  ? -columns
                                  : event.key === "ArrowDown"
                                    ? columns
                                    : 0;
                          if (!delta || locked || dragRef.current) return;
                          event.preventDefault();
                          const targetIndex = index + delta;
                          const target = order[targetIndex];
                          if (target === undefined) return;
                          focusIdRef.current = id;
                          setOrder((previous) => reorderAssetIdsByDrop(previous, id, target));
                          virtualizer.scrollToIndex(Math.floor(targetIndex / columns), {
                            align: "auto"
                          });
                        }}
                      >
                        <UiIcon name="grip-vertical" className="h-5 w-5" />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
          {showInsertionLine && targetRow ? (
            <div
              aria-hidden="true"
              data-testid="bulk-order-insertion-line"
              className="pointer-events-none absolute z-10 w-1 rounded-full bg-primary"
              style={{
                top: targetRow.start + 2,
                height: rowHeight - 16,
                left: Math.max(
                  0,
                  Math.min(
                    width - 4,
                    (targetIndex % columns) * (tileWidth + 12) + (insertAfter ? tileWidth + 4 : -8)
                  )
                )
              }}
            />
          ) : null}
        </div>
      </div>
      {draggedAsset ? (
        <div
          ref={ghostRef}
          aria-hidden="true"
          className="pointer-events-none fixed left-0 top-0 z-[70] h-28 w-28 overflow-hidden rounded-[var(--radius-control)] border-2 border-primary bg-base-200 opacity-90 shadow-lg"
        >
          <ThumbnailSubscription
            id={draggedAsset.id}
            path={thumbs[draggedAsset.id]}
            fit="contain"
          />
        </div>
      ) : null}
      <span className="sr-only" aria-live="polite">
        {announcement}
      </span>
      <footer className="grid shrink-0 gap-3">
        {result?.status === "failed" || result?.status === "ignored" ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.groupSaveFailed")}
          </UiAlert>
        ) : result?.status === "partial" ? (
          <UiAlert tone="warning" title={t("bulk.panel.partialResult", result)}>
            {t("bulk.orderModal.partial")}
          </UiAlert>
        ) : null}
        <div className="flex justify-end gap-2">
          <UiButton disabled={locked} onClick={onClose}>
            {t("common.cancel")}
          </UiButton>
          <UiButton
            variant="primary"
            disabled={locked || controller.selectionBusy || controller.metadataLoading || controller.metadataFailed}
            aria-busy={saving}
            onClick={() => void save()}
          >
            {saving ? t("bulk.groupModal.applyInProgress") : t("bulk.orderModal.save")}
          </UiButton>
        </div>
      </footer>
    </UiModal>
  );
}
