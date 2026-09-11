import { useCallback, memo, useSyncExternalStore, type MouseEvent } from "react";
import { toMediaSrc } from "../../api";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../UI/ThumbnailImage";
import { UiIcon } from "../UI/UiIcon";
import { UiChip } from "../UI/UiChip";
import type { AssetSummary } from "../../types";
import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";

const TilePreview = memo(function TilePreview({
  asset,
  thumbPath,
  showRenderLoader
}: {
  asset: AssetSummary;
  thumbPath?: string;
  showRenderLoader: boolean;
}) {
  const src = thumbPath ? toMediaSrc(thumbPath) : TRANSPARENT_THUMBNAIL_SRC;

  return (
    <>
      <ThumbnailImage
        className="block h-full w-full object-cover"
        src={src}
        alt={asset.preview_path ?? asset.file_name}
      />
      {showRenderLoader ? (
        <div
          className="absolute inset-0 grid place-items-center bg-base-100/60"
          aria-hidden="true"
          data-testid="tile-thumb-loader"
        >
          <span className="h-[26px] w-[26px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
        </div>
      ) : null}
    </>
  );
});

interface GalleryTileProps {
  itemIndex: number;
  itemLane: number;
  itemStart: number;
  tilePixelSize: number;
  tileGap: number;
  asset: AssetSummary;
  thumbPath?: string;
  showRenderLoader: boolean;
  isBulkSelected: boolean;
  isLightboxSelected: boolean;
  groupedDescription: string;
  videoChipLabel: string;
  gifChipLabel: string;
  thumbnailStore?: ThumbnailStore;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}

export const GalleryTile = memo(function GalleryTile({
  itemIndex,
  itemLane,
  itemStart,
  tilePixelSize,
  tileGap,
  asset,
  thumbPath,
  showRenderLoader,
  isBulkSelected,
  isLightboxSelected,
  groupedDescription,
  videoChipLabel,
  gifChipLabel,
  thumbnailStore,
  onClick
}: GalleryTileProps) {
  const isGrouped = Boolean(asset.media_group_key?.trim());
  const subscribe = useCallback((listener: () => void) =>
    thumbnailStore ? thumbnailStore.subscribe(asset.id, listener) : () => {},
    [thumbnailStore, asset.id]);
  const getSnapshot = useCallback(() => thumbnailStore?.getVersion(asset.id) ?? 0,
    [thumbnailStore, asset.id]);
  useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const effectiveThumbPath = thumbnailStore?.getPath(asset.id) ?? thumbPath;
  const effectiveRenderLoader = thumbnailStore
    ? thumbnailStore.isRendering(asset.id) && !effectiveThumbPath
    : showRenderLoader;

  return (
    <button
      className={`absolute z-[1] overflow-hidden rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-base-300/95 p-0 ${
        isBulkSelected
          ? "shadow-[0_0_0_2px_var(--color-primary),var(--shadow-tile-hover)]"
          : isLightboxSelected
            ? "shadow-[0_0_0_2px_var(--color-primary),var(--shadow-tile-hover)]"
            : "shadow-[var(--shadow-tile)] hover:shadow-[var(--shadow-tile-hover)]"
      }`}
      style={{
        width: tilePixelSize,
        height: tilePixelSize,
        transform: `translate3d(${itemLane * (tilePixelSize + tileGap)}px, ${itemStart}px, 0)`
      }}
      data-asset-id={asset.id}
      data-asset-index={itemIndex}
      draggable={false}
      onClick={onClick}
      aria-pressed={isBulkSelected || isLightboxSelected}
    >
      <TilePreview
        asset={asset}
        thumbPath={effectiveThumbPath}
        showRenderLoader={effectiveRenderLoader}
      />
      {isGrouped ? <span className="sr-only">{groupedDescription}</span> : null}
      <span className="gallery-selection-mark pointer-events-none absolute left-2 top-2 grid h-6 w-6 place-items-center rounded-[var(--radius-control)] bg-primary text-primary-content" style={{ opacity: isBulkSelected ? 1 : 0 }} aria-hidden="true">
        <UiIcon name="check-square" className="h-4 w-4" />
      </span>
      {asset.kind === "video" ? (
        <UiChip className="pointer-events-none absolute right-2 top-2 shadow-[var(--shadow-chip)]" tone="video">
          {videoChipLabel}
        </UiChip>
      ) : null}
      {asset.kind === "gif" ? (
        <UiChip className="pointer-events-none absolute right-2 top-2 shadow-[var(--shadow-chip)]" tone="gif">
          {gifChipLabel}
        </UiChip>
      ) : null}
    </button>
  );
});
