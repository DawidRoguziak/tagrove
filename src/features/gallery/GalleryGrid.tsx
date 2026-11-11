import { createEffect, createMemo, createSignal, For, onCleanup, Repeat, Show } from "solid-js";
import { Portal } from "@solidjs/web";
import type { AssetQuerySection, AssetSummary } from "../../platform/tauri/contracts/assets";
import type { BulkSelectionHandler } from "./selection";
import type { LibraryViewport } from "../../library/createLibrary";
import { useLanguage } from "../../i18n/solid";
import { GalleryEmptyState } from "./GalleryEmptyState";
import { GalleryScrollbar } from "./GalleryScrollbar";
import { GalleryStatusFooter } from "./GalleryStatusFooter";
import { GalleryTimelineScrubber, TIMELINE_WIDTH } from "./GalleryTimelineScrubber";
import { formatTimelineMonth, timelineMonths } from "./timeline";
import { GalleryTile } from "./GalleryTile";
import { createVirtualGrid } from "./createVirtualGrid";
import { createGridHandlers } from "./createGridHandlers";
export interface GalleryGridProps {
  assets: AssetSummary[];
  assetCount: number;
  getAssetAt: (index: number) => AssetSummary | undefined;
  tileSize: number;
  scroller: () => HTMLElement | undefined;
  onVirtualRangeChange?: (viewport: LibraryViewport | null) => void;
  onCtrlWheelZoom?: (delta: number) => void;
  onSelect: (asset: AssetSummary) => void;
  hasScanRoots: boolean;
  onAddFirstFolder: () => void;
  selectionModeEnabled: boolean;
  /** Selection lookup by global index and asset ID (no ID for a placeholder tile). */
  isSelected: (index: number, id?: number) => boolean;
  onBulkSelectionInteraction: BulkSelectionHandler;
  loadError: string | null;
  onLoadRetry: () => void;
  queryEpoch: number;
  pageFailureEpoch: number;
  /** Timeline sections of the current query session (stage 08). */
  sections?: readonly AssetQuerySection[];
}
interface Slot {
  id: number;
}
const CONTENT_ID = "gallery-grid-content";
/**
 * Right offset of the timeline rail, clear of the position bar in both scroll modes. WebKitGTK's
 * overlay scrollbar takes the pointer hits of the right 21 px (measured in the E2E build).
 */
const TIMELINE_RIGHT = 24;
/**
 * The rail (`TIMELINE_WIDTH` wide, `TIMELINE_RIGHT` from the edge) spans [W-34, W-24]. The tile
 * area's right padding (inline style on the gallery section) keeps every column out of it, so the
 * rail never takes a tile's pointer hits. Geometry follows `grid.clientWidth`, so columns
 * adapt without further bookkeeping.
 */
const TIMELINE_CLEARANCE = TIMELINE_RIGHT + TIMELINE_WIDTH;
export function GalleryGrid(props: GalleryGridProps) {
  const { t, language } = useLanguage();
  const [gallery, setGallery] = createSignal<HTMLElement>();
  const [grid, setGrid] = createSignal<HTMLDivElement>();
  const virtual = createVirtualGrid({
    get assetCount() {
      return props.assetCount;
    },
    get tileSize() {
      return props.tileSize;
    },
    gallery,
    grid,
    scroller: () => props.scroller(),
    onRange: (range) => props.onVirtualRangeChange?.(range),
    get rangeResetKey() {
      return props.pageFailureEpoch;
    }
  });
  const handlers = createGridHandlers({
    get assets() {
      return props.assets;
    },
    get assetCount() {
      return props.assetCount;
    },
    get queryEpoch() {
      return props.queryEpoch;
    },
    gridRef: grid,
    scrollContainerRef: () => props.scroller(),
    get gridOffset() {
      return virtual.gridOffset;
    },
    get columnCount() {
      return virtual.columnCount;
    },
    get tilePixelSize() {
      return virtual.tilePixelSize;
    },
    get tileGap() {
      return virtual.tileGap;
    },
    get scrollShift() {
      return virtual.scrollShift;
    },
    scrollBy: virtual.scrollBy,
    getAssetAt: (index) => props.getAssetAt(index),
    get selectionModeEnabled() {
      return props.selectionModeEnabled;
    },
    onCtrlWheelZoom: (delta) => props.onCtrlWheelZoom?.(delta),
    onSelect: (asset) => props.onSelect(asset),
    onBulkSelectionInteraction: (interaction) => props.onBulkSelectionInteraction(interaction)
  });
  createEffect(gallery, (element) => {
    element?.addEventListener("click", handlers.handleClickCapture, true);
    return () => element?.removeEventListener("click", handlers.handleClickCapture, true);
  });
  // A fixed pool of row slots. Slot `id` shows the row congruent to it modulo the pool,
  // so crossing a row re-indexes one slot instead of creating components. Slots are
  // ordered by row, keeping DOM order equal to visual order for focus and screen readers.
  const slots = createMemo((previous: Slot[] | undefined) => {
    const size = virtual.poolSize;
    if (previous?.length === size) return previous;
    const next = (previous ?? []).slice(0, size);
    for (let id = next.length; id < size; id++) next.push({ id });
    return next;
  });
  const orderedSlots = createMemo(() => {
    const all = slots();
    if (!all.length) return all;
    const start = virtual.firstRow % all.length;
    return start ? [...all.slice(start), ...all.slice(0, start)] : all;
  });
  const months = createMemo(() => timelineMonths(props.sections ?? []));
  const assetAt = (index: number) => {
    props.assets;
    return index < props.assetCount ? props.getAssetAt(index) : undefined;
  };
  const rectangleStyle = () => {
    const rect = handlers.preview?.rectangle;
    return rect
      ? {
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`
        }
      : {};
  };
  function Row(rowProps: { slot: Slot }) {
    const row = createMemo(() => {
      const size = slots().length;
      const first = virtual.firstRow;
      return first + ((((rowProps.slot.id - first) % size) + size) % size);
    });
    const active = () => row() < virtual.rowCount;
    const gap = () => virtual.tileGap;
    // Media-group backplates for runs of adjacent tiles in this row.
    const plates = createMemo(() => {
      const result: { first: number; last: number }[] = [];
      if (!active()) return result;
      const columns = virtual.columnCount;
      let previousKey: string | undefined;
      for (let lane = 0; lane < columns; lane++) {
        const key = assetAt(row() * columns + lane)?.media_group_key?.trim() || undefined;
        const last = result[result.length - 1];
        if (key && key === previousKey && last && last.last === lane - 1) last.last = lane;
        else if (key) result.push({ first: lane, last: lane });
        previousKey = key;
      }
      return result;
    });
    return (
      <div
        class="gallery-row absolute"
        data-gallery-row={row()}
        hidden={!active()}
        style={{
          // The box includes half a gap on every side so strict containment does not
          // clip group backplates (4px) or the selection ring (2px).
          left: `${-gap() / 2}px`,
          right: `${-gap() / 2}px`,
          top: "0",
          height: `${virtual.tilePixelSize + gap()}px`,
          padding: `${gap() / 2}px`,
          transform: `translate(0, ${row() * (virtual.tilePixelSize + gap()) - virtual.scrollShift - gap() / 2}px)`,
          contain: "strict",
          display: "grid",
          "grid-template-columns": `repeat(${virtual.columnCount}, ${virtual.tilePixelSize}px)`,
          "grid-auto-rows": `${virtual.tilePixelSize}px`,
          "column-gap": `${gap()}px`
        }}
      >
        <For each={plates()} keyed={false}>
          {(plate) => (
            <div
              class="pointer-events-none absolute z-0 rounded-[calc(var(--radius-surface)+4px)] bg-[var(--gallery-group-bg)] shadow-[inset_0_0_0_1px_var(--gallery-group-edge)]"
              aria-hidden="true"
              data-gallery-group-plate
              style={{
                top: `${gap() / 2 - 4}px`,
                left: `${gap() / 2 + plate().first * (virtual.tilePixelSize + gap()) - 4}px`,
                width: `${(plate().last - plate().first + 1) * virtual.tilePixelSize + (plate().last - plate().first) * gap() + 8}px`,
                height: `${virtual.tilePixelSize + 8}px`
              }}
            />
          )}
        </For>
        <Repeat count={virtual.columnCount}>
          {(lane) => {
            const index = createMemo(() => row() * virtual.columnCount + lane);
            const asset = createMemo(() => (active() ? assetAt(index()) : undefined));
            // The tile reads the slot's last shown asset, never the inner Show's narrowed
            // accessor: a recycled tile's late image load or error would read that accessor
            // after the slot moved to an unloaded index and throw "Stale read from <Show>".
            const tileAsset = createMemo<AssetSummary | undefined>(
              (previous) => asset() ?? previous
            );
            const selected = () => {
              const current = asset();
              return (
                props.selectionModeEnabled &&
                handlers.isPreviewSelected(index(), props.isSelected(index(), current?.id))
              );
            };
            return (
              <Show when={active() && index() < props.assetCount}>
                <Show
                  when={asset()}
                  fallback={
                    <div
                      data-asset-index={index()}
                      aria-hidden="true"
                      class={`relative z-[1] ${virtual.paused ? "" : "animate-pulse"} rounded-[var(--radius-surface)] bg-base-300/70 ${selected() ? "ring-2 ring-primary" : ""}`}
                    />
                  }
                >
                  <GalleryTile
                    // Defined: the Show renders the tile only while `asset()` is.
                    asset={tileAsset() as AssetSummary}
                    index={index()}
                    selected={selected()}
                    thumbnailsPaused={virtual.paused}
                    prefetch={
                      index() < virtual.range.visibleStartIndex ||
                      index() > virtual.range.visibleEndIndex
                    }
                    onClick={handlers.handleTileClick}
                  />
                </Show>
              </Show>
            );
          }}
        </Repeat>
      </div>
    );
  }
  function GridContent() {
    // The empty state unmounts the content. Without its element the virtual grid and the
    // selection handlers drop their observers at once, instead of measuring a detached
    // node from inside a ResizeObserver delivery.
    onCleanup(() => setGrid(undefined));
    return (
      <div
        ref={setGrid}
        id={CONTENT_ID}
        data-testid="gallery-grid-content"
        class="relative"
        style={{ height: `${virtual.physicalHeight}px`, "overflow-anchor": "none" }}
      >
        <For each={orderedSlots()}>{(slot) => <Row slot={slot} />}</For>
      </div>
    );
  }
  return (
    <div class="relative flex min-w-0 flex-col">
      <Show when={virtual.windowed && props.assetCount > 0}>
        <GalleryScrollbar
          controls={CONTENT_ID}
          label={t("gallery.scrollbarLabel")}
          viewport={virtual.viewport}
          offset={virtual.offset}
          maxOffset={virtual.maxOffset}
          stride={virtual.tilePixelSize + virtual.tileGap}
          firstVisible={virtual.range.visibleStartIndex}
          count={props.assetCount}
          onScrollTo={(offset) => virtual.scrollTo(offset, true)}
        />
      </Show>
      {/* Month navigation beside the position bar. GTK's overlay scrollbar (native mode) and
          `GalleryScrollbar` (windowed) both take pointer hits in the right 14-21px (see TIMELINE_RIGHT). */}
      <Show when={props.assetCount > 0 && months().length > 1}>
        <GalleryTimelineScrubber
          controls={CONTENT_ID}
          label={t("gallery.timelineLabel")}
          months={months()}
          total={props.assetCount}
          viewport={virtual.viewport}
          firstVisible={virtual.range.visibleStartIndex}
          right={TIMELINE_RIGHT}
          formatMonth={(month) => formatTimelineMonth(month, language(), t("gallery.timelineNoDate"))}
          onJump={(index) => virtual.scrollToIndex(index)}
          onHold={virtual.holdDemand}
          onRelease={virtual.resumeDemand}
        />
      </Show>
      <section
        class={`min-h-0 flex-1 min-w-0 w-full pl-3 pt-3 sm:pl-5 sm:pt-5 ${props.selectionModeEnabled ? (handlers.isDragSelecting ? "cursor-crosshair select-none" : "select-none") : ""}`}
        ref={setGallery}
        onWheel={handlers.handleGalleryWheel}
        onPointerDown={handlers.handlePointerDown}
        onPointerMove={handlers.handlePointerMove}
        onPointerUp={handlers.handlePointerUp}
        onPointerCancel={handlers.handlePointerCancel}
        onLostPointerCapture={handlers.handleLostPointerCapture}
        onDragStart={(event) => event.preventDefault()}
        style={{
          "padding-right": `${TIMELINE_CLEARANCE}px`,
          "touch-action": props.selectionModeEnabled ? "none" : undefined
        }}
        data-testid="gallery-grid"
        data-scroll-paused={virtual.paused}
      >
        <Show when={handlers.preview}>
          <Portal>
            <div
              data-testid="gallery-selection-rectangle"
              aria-hidden="true"
              class="pointer-events-none fixed z-20 border border-primary bg-primary/20"
              style={rectangleStyle()}
            />
          </Portal>
        </Show>
        {props.loadError ? (
          <div
            role="alert"
            data-testid="gallery-load-error"
            class="mb-3 flex items-center justify-between gap-3 rounded-[var(--radius-surface)] border border-error/40 bg-error/10 px-4 py-2 text-sm"
          >
            <span>{t("gallery.loadError")}</span>
            <button type="button" class="btn btn-xs btn-outline" onClick={props.onLoadRetry}>
              {t("gallery.retry")}
            </button>
          </div>
        ) : null}
        <Show
          when={props.assetCount > 0}
          fallback={
            <GalleryEmptyState
              hasScanRoots={props.hasScanRoots}
              onAddFirstFolder={props.onAddFirstFolder}
              noFoldersTitle={t("gallery.noFoldersTitle")}
              noFoldersDescription={t("gallery.noFoldersDescription")}
              addFirstFolderLabel={t("gallery.addFirstFolder")}
              noResultsTitle={t("gallery.noResultsTitle")}
              noResultsDescription={t("gallery.noResultsDescription")}
            />
          }
        >
          <GridContent />
        </Show>
      </section>
      {props.assetCount > 0 ? (
        <div class="pointer-events-none">
          <GalleryStatusFooter resultCount={props.assetCount} />
        </div>
      ) : null}
    </div>
  );
}
