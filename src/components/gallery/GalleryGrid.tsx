import { memo, useRef } from "react";
import { createPortal } from "react-dom";
import type { RefObject } from "react";
import type { AssetSummary } from "../../types";
import { GalleryEmptyState } from "./GalleryEmptyState";
import { GalleryStatusFooter } from "./GalleryStatusFooter";
import { GalleryTile } from "./GalleryTile";
import { useGalleryGridHandlers } from "./hooks/useGalleryGridHandlers";
import { useGalleryVirtualGrid, type GalleryRange } from "./hooks/useGalleryVirtualGrid";
import { useTranslation } from "react-i18next";
import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";
import type { BulkSelectionHandler } from "./selection";

const EMPTY_SELECTED_IDS = new Set<number>();
const GROUP_BACKPLATE_EDGE = 4;

function getMediaGroupKey(asset: AssetSummary | undefined): string | null {
  const mediaGroupKey = asset?.media_group_key?.trim();
  return mediaGroupKey || null;
}

interface GalleryGroupBackplate {
  groupKey: string;
  startIndex: number;
  endIndex: number;
  startLane: number;
  endLane: number;
  itemStart: number;
}


interface GalleryGridProps {
  assets: AssetSummary[];
  assetCount?: number;
  getAssetAt?: (index: number) => AssetSummary | undefined;
  selectedId: number | null;
  thumbs: Record<number, string>;
  tileSize: number;
  hasMore: boolean;
  isLoading: boolean;
  isGeneratingThumbnails: boolean;
  pendingThumbnailCount: number;
  renderingThumbnailIds: Record<number, true>;
  thumbnailStore?: ThumbnailStore;
  scrollContainerRef?: RefObject<HTMLElement | null>;
  onReachEnd: () => void;
  onVirtualRangeChange?: (range: GalleryRange) => void;
  onCtrlWheelZoom?: (deltaY: number) => void;
  onSelect: (asset: AssetSummary) => void;
  hasScanRoots?: boolean;
  onAddFirstFolder?: () => void;
  selectionModeEnabled?: boolean;
  selectedAssetIds?: Set<number>;
  onBulkSelectionInteraction?: BulkSelectionHandler;
  loadError?: string | null;
  onLoadRetry?: () => void;
  queryEpoch?: number;
  pageFailureEpoch?: number;
}

const GalleryGridContent = memo(function GalleryGridContent({
  assets,
  assetCount,
  getAssetAt,
  selectedId,
  thumbs,
  tileSize,
  hasMore,
  isLoading,
  renderingThumbnailIds,
  thumbnailStore,
  scrollContainerRef,
  onReachEnd,
  onVirtualRangeChange,
  onCtrlWheelZoom,
  onSelect,
  hasScanRoots = true,
  onAddFirstFolder,
  selectionModeEnabled = false,
  selectedAssetIds = EMPTY_SELECTED_IDS,
  onBulkSelectionInteraction,
  loadError = null,
  onLoadRetry,
  queryEpoch = 0,
  pageFailureEpoch = 0
}: Omit<GalleryGridProps, "isGeneratingThumbnails" | "pendingThumbnailCount">) {
  const resolvedAssetCount = assetCount ?? assets.length;
  const resolvedGetAssetAt = getAssetAt ?? ((index: number) => assets[index]);
  const { t } = useTranslation();
  const groupedDescription = t("gallery.groupedChip");
  const videoChipLabel = t("gallery.videoChip");
  const gifChipLabel = t("gallery.gifChip");
  const galleryRef = useRef<HTMLDivElement | null>(null);
  const gridRef = useRef<HTMLDivElement | null>(null);

  const virtualGrid = useGalleryVirtualGrid({
    assetCount: resolvedAssetCount,
    getAssetAt: resolvedGetAssetAt,
    tileSize,
    hasMore,
    isLoading,
    galleryRef,
    gridRef,
    scrollContainerRef,
    onReachEnd,
    onVirtualRangeChange,
    rangeResetKey: pageFailureEpoch
  });
  const handlers = useGalleryGridHandlers({
    assets,
    assetCount: resolvedAssetCount,
    queryEpoch,
    gridRef,
    scrollContainerRef,
    gridOffset: virtualGrid.gridOffset,
    columnCount: virtualGrid.columnCount,
    tilePixelSize: virtualGrid.tilePixelSize,
    tileGap: virtualGrid.tileGap,
    getAssetAt: resolvedGetAssetAt,
    selectionModeEnabled,
    onCtrlWheelZoom,
    onSelect,
    onBulkSelectionInteraction
  });

  const virtualEntries = virtualGrid.virtualItems.map((item) => ({
    asset: resolvedGetAssetAt(item.index),
    item,
    itemLane: item.lane
  }));
  const groupBackplates: GalleryGroupBackplate[] = [];

  for (const { asset, item, itemLane } of virtualEntries) {
    const groupKey = getMediaGroupKey(asset);
    if (!groupKey) {
      continue;
    }

    const currentBackplate = groupBackplates[groupBackplates.length - 1];
    if (
      currentBackplate &&
      currentBackplate.groupKey === groupKey &&
      currentBackplate.endIndex + 1 === item.index &&
      currentBackplate.endLane + 1 === itemLane &&
      currentBackplate.itemStart === item.start
    ) {
      currentBackplate.endIndex = item.index;
      currentBackplate.endLane = itemLane;
      continue;
    }

    groupBackplates.push({
      groupKey,
      startIndex: item.index,
      endIndex: item.index,
      startLane: itemLane,
      endLane: itemLane,
      itemStart: item.start
    });
  }

  return (
    <section
      className={`min-h-0 flex-1 min-w-0 w-full px-3 pb-16 pt-3 sm:px-5 sm:pt-5 ${
        selectionModeEnabled ? (handlers.isDragSelecting ? "cursor-crosshair select-none" : "select-none") : ""
      }`}
      ref={galleryRef}
      onWheel={handlers.handleGalleryWheel}
      onPointerDown={handlers.handlePointerDown}
      onPointerMove={handlers.handlePointerMove}
      onPointerUp={handlers.handlePointerUp}
      onPointerCancel={handlers.handlePointerCancel}
      onLostPointerCapture={handlers.handleLostPointerCapture}
      onClickCapture={handlers.handleClickCapture}
      onClick={handlers.handleGalleryClick}
      onDragStart={(event) => event.preventDefault()}
      style={{ touchAction: selectionModeEnabled ? "none" : undefined }}
      data-testid="gallery-grid"
    >
      {handlers.preview ? createPortal(
        <div data-testid="gallery-selection-rectangle" aria-hidden="true"
          className="pointer-events-none fixed z-20 border border-primary bg-primary/20"
          style={handlers.preview.rectangle} />, document.body
      ) : null}
      {loadError ? (
        <div
          role="alert"
          data-testid="gallery-load-error"
          className="mb-3 flex items-center justify-between gap-3 rounded-[var(--radius-surface)] border border-error/40 bg-error/10 px-4 py-2 text-sm"
        >
          <span>{t("gallery.loadError")}</span>
          {onLoadRetry ? (
            <button type="button" className="btn btn-xs btn-outline" onClick={onLoadRetry}>
              {t("gallery.retry")}
            </button>
          ) : null}
        </div>
      ) : null}
      {!resolvedAssetCount ? (
        <GalleryEmptyState
          hasScanRoots={hasScanRoots}
          onAddFirstFolder={onAddFirstFolder}
          noFoldersTitle={t("gallery.noFoldersTitle")}
          noFoldersDescription={t("gallery.noFoldersDescription")}
          addFirstFolderLabel={t("gallery.addFirstFolder")}
          noResultsTitle={t("gallery.noResultsTitle")}
          noResultsDescription={t("gallery.noResultsDescription")}
        />
      ) : (
          <div ref={gridRef} style={{ height: virtualGrid.totalSize, position: "relative" }}>
            {groupBackplates.map((backplate) => {
              const tileCount = backplate.endLane - backplate.startLane + 1;
              return (
                <div
                  key={`group-backplate-${backplate.startIndex}`}
                  className="pointer-events-none absolute z-0 rounded-[calc(var(--radius-surface)+4px)] bg-primary/16"
                  style={{
                    width:
                      tileCount * virtualGrid.tilePixelSize +
                      (tileCount - 1) * virtualGrid.tileGap +
                      GROUP_BACKPLATE_EDGE * 2,
                    height: virtualGrid.tilePixelSize + GROUP_BACKPLATE_EDGE * 2,
                    transform: `translate3d(${backplate.startLane * (virtualGrid.tilePixelSize + virtualGrid.tileGap) - GROUP_BACKPLATE_EDGE}px, ${backplate.itemStart - GROUP_BACKPLATE_EDGE}px, 0)`
                  }}
                  aria-hidden="true"
                />
              );
            })}
            {virtualEntries.map(({ asset, item, itemLane }) => {
              if (!asset) {
                return (
                  <div
                    key={`pending-${item.index}`}
                    data-asset-index={item.index}
                    className={`absolute z-[1] animate-pulse rounded-[var(--radius-surface)] bg-base-300/70 ${handlers.isPreviewSelected(item.index, false) ? "ring-2 ring-primary" : ""}`}
                    style={{
                      width: virtualGrid.tilePixelSize,
                      height: virtualGrid.tilePixelSize,
                      transform: `translate3d(${itemLane * (virtualGrid.tilePixelSize + virtualGrid.tileGap)}px, ${item.start}px, 0)`
                    }}
                    aria-hidden="true"
                  />
                );
              }

              const showRenderLoader = Boolean(renderingThumbnailIds[asset.id]) && !thumbs[asset.id];
              const isBulkSelected = selectionModeEnabled && handlers.isPreviewSelected(item.index, selectedAssetIds.has(asset.id));
              const isLightboxSelected = !selectionModeEnabled && selectedId === asset.id;

              return (
                <GalleryTile
                  key={asset.id}
                  itemIndex={item.index}
                  itemLane={itemLane}
                  itemStart={item.start}
                  tilePixelSize={virtualGrid.tilePixelSize}
                  tileGap={virtualGrid.tileGap}
                  asset={asset}
                  thumbPath={thumbs[asset.id]}
                  showRenderLoader={showRenderLoader}
                  isBulkSelected={isBulkSelected}
                  isLightboxSelected={isLightboxSelected}
                  groupedDescription={groupedDescription}
                  videoChipLabel={videoChipLabel}
                  gifChipLabel={gifChipLabel}
                  thumbnailStore={thumbnailStore}
                  onClick={handlers.handleTileClick}
                />
              );
            })}
          </div>
      )}
    </section>
  );
});

export const GalleryGrid = memo(function GalleryGrid({
  isGeneratingThumbnails, pendingThumbnailCount, ...contentProps
}: GalleryGridProps) {
  const { t } = useTranslation();
  return (
    <div className="relative flex min-w-0 flex-col">
      <GalleryGridContent {...contentProps} />
      {(contentProps.assetCount ?? contentProps.assets.length) > 0 ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-0">
          <GalleryStatusFooter
            isGeneratingThumbnails={isGeneratingThumbnails}
            hasMore={contentProps.hasMore}
            generatingLabel={t("gallery.generatingThumbnails", { count: pendingThumbnailCount })}
            noMoreLabel={t("gallery.noMoreItems")}
          />
        </div>
      ) : null}
    </div>
  );
});
