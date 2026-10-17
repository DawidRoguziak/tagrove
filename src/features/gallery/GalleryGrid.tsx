import { createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { Portal } from "@solidjs/web";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import type { ThumbnailStore } from "../../library/thumbnails/thumbnailStore";
import type { BulkSelectionHandler } from "./selection";
import type { LibraryViewport } from "../../library/createLibrary";
import { useLanguage } from "../../i18n/solid";
import { GalleryEmptyState } from "./GalleryEmptyState";
import { GalleryStatusFooter } from "./GalleryStatusFooter";
import { GalleryTile } from "./GalleryTile";
import { createVirtualGrid } from "./createVirtualGrid";
import { createGridHandlers } from "./createGridHandlers";
export interface GalleryGridProps {
  assets: AssetSummary[];
  assetCount: number;
  getAssetAt: (index: number) => AssetSummary | undefined;
  tileSize: number;
  hasMore: boolean;
  isLoading: boolean;
  isGeneratingThumbnails: boolean;
  pendingThumbnailCount: number;
  thumbnailStore: ThumbnailStore;
  scroller: () => HTMLElement | undefined;
  onReachEnd: () => void;
  onVirtualRangeChange?: (viewport: LibraryViewport | null) => void;
  onCtrlWheelZoom?: (delta: number) => void;
  onSelect: (asset: AssetSummary) => void;
  hasScanRoots: boolean;
  onAddFirstFolder: () => void;
  selectionModeEnabled: boolean;
  selectedAssetIds: ReadonlySet<number>;
  onBulkSelectionInteraction: BulkSelectionHandler;
  loadError: string | null;
  onLoadRetry: () => void;
  queryEpoch: number;
  pageFailureEpoch: number;
}
export function GalleryGrid(props: GalleryGridProps) {
  const { t } = useLanguage();
  const [gallery, setGallery] = createSignal<HTMLElement>();
  const [grid, setGrid] = createSignal<HTMLDivElement>();
  const virtual = createVirtualGrid({
    get assetCount() {
      return props.assetCount;
    },
    get tileSize() {
      return props.tileSize;
    },
    get hasMore() {
      return props.hasMore;
    },
    get isLoading() {
      return props.isLoading;
    },
    gallery,
    grid,
    scroller: () => props.scroller(),
    onReachEnd: () => props.onReachEnd(),
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
  const entries = createMemo(() => {
    if (virtual.paused) return [];
    props.assets;
    return virtual.virtualItems.map((item) => ({ ...item, asset: props.getAssetAt(item.index) }));
  });
  const backplates = createMemo(() => {
    const result: {
      key: string;
      first: number;
      last: number;
      lane: number;
      endLane: number;
      start: number;
    }[] = [];
    for (const entry of entries()) {
      const key = entry.asset?.media_group_key?.trim();
      if (!key) continue;
      const last = result[result.length - 1];
      if (
        last &&
        last.key === key &&
        last.last + 1 === entry.index &&
        last.endLane + 1 === entry.lane &&
        last.start === entry.start
      ) {
        last.last = entry.index;
        last.endLane = entry.lane;
      } else
        result.push({
          key,
          first: entry.index,
          last: entry.index,
          lane: entry.lane,
          endLane: entry.lane,
          start: entry.start
        });
    }
    return result;
  });
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
  return (
    <div class="relative flex min-w-0 flex-col">
      <section
        class={`min-h-0 flex-1 min-w-0 w-full px-3 pt-3 sm:px-5 sm:pt-5 ${props.selectionModeEnabled ? (handlers.isDragSelecting ? "cursor-crosshair select-none" : "select-none") : ""}`}
        ref={setGallery}
        onWheel={handlers.handleGalleryWheel}
        onPointerDown={handlers.handlePointerDown}
        onPointerMove={handlers.handlePointerMove}
        onPointerUp={handlers.handlePointerUp}
        onPointerCancel={handlers.handlePointerCancel}
        onLostPointerCapture={handlers.handleLostPointerCapture}
        onDragStart={(event) => event.preventDefault()}
        style={{ "touch-action": props.selectionModeEnabled ? "none" : undefined }}
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
          <div
            ref={setGrid}
            style={{
              height: `${virtual.totalSize}px`,
              position: "relative",
              "overflow-anchor": "none"
            }}
          >
            <For each={backplates()} keyed={false}>
              {(plate) => (
                <div
                  class="pointer-events-none absolute z-0 rounded-[calc(var(--radius-surface)+4px)] bg-[var(--gallery-group-bg)] shadow-[inset_0_0_0_1px_var(--gallery-group-edge)]"
                  aria-hidden="true"
                  style={{
                    width: `${(plate().endLane - plate().lane + 1) * virtual.tilePixelSize + (plate().endLane - plate().lane) * virtual.tileGap + 8}px`,
                    height: `${virtual.tilePixelSize + 8}px`,
                    transform: `translate3d(${plate().lane * (virtual.tilePixelSize + virtual.tileGap) - 4}px, ${plate().start - 4}px, 0)`
                  }}
                />
              )}
            </For>
            <For each={virtual.virtualItems.map((item) => item.index)}>
              {(index) => {
                const assetAtSlot = createMemo(() => {
                  props.assets;
                  return props.getAssetAt(index);
                });
                const selectedAtSlot = () => {
                  const asset = assetAtSlot();
                  return props.selectionModeEnabled && handlers.isPreviewSelected(
                    index, !!asset && props.selectedAssetIds.has(asset.id)
                  );
                };
                const lane = () => index % virtual.columnCount;
                const start = () =>
                  Math.floor(index / virtual.columnCount) *
                    (virtual.tilePixelSize + virtual.tileGap) -
                  virtual.scrollShift;
                return (
                  <Show
                    when={!virtual.paused && assetAtSlot()}
                    fallback={
                      <div
                        data-asset-index={index}
                        aria-hidden="true"
                        class={`absolute z-[1] ${virtual.paused ? "" : "animate-pulse"} rounded-[var(--radius-surface)] bg-base-300/70 ${selectedAtSlot() ? "ring-2 ring-primary" : ""}`}
                        style={{
                          width: `${virtual.tilePixelSize}px`,
                          height: `${virtual.tilePixelSize}px`,
                          transform: `translate3d(${lane() * (virtual.tilePixelSize + virtual.tileGap)}px, ${start()}px, 0)`
                        }}
                      />
                    }
                  >
                    {(asset) => (
                      <GalleryTile
                        asset={asset()}
                        index={index}
                        lane={lane()}
                        start={start()}
                        size={virtual.tilePixelSize}
                        gap={virtual.tileGap}
                        selected={selectedAtSlot()}
                        thumbnailStore={props.thumbnailStore}
                        onClick={handlers.handleTileClick}
                      />
                    )}
                  </Show>
                );
              }}
            </For>
          </div>
        </Show>
      </section>
      {props.assetCount > 0 ? (
        <div class="pointer-events-none">
          <GalleryStatusFooter
            isGeneratingThumbnails={props.isGeneratingThumbnails}
            hasMore={props.hasMore}
            generatingLabel={t("gallery.generatingThumbnails", {
              count: props.pendingThumbnailCount
            })}
            resultCount={props.assetCount}
          />
        </div>
      ) : null}
    </div>
  );
}
