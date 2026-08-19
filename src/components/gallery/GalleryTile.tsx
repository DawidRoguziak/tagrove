import { memo, useSyncExternalStore, type MouseEvent } from "react";
import { toMediaSrc } from "../../api";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../UI/ThumbnailImage";
import { UiChip } from "../UI/UiChip";
import type { Asset } from "../../types";
import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";

const TilePreview = memo(function TilePreview({
  asset,
  thumbPath,
  shouldAnimateGif,
  showRenderLoader
}: {
  asset: Asset;
  thumbPath?: string;
  shouldAnimateGif: boolean;
  showRenderLoader: boolean;
}) {
  const src =
    asset.kind === "gif"
      ? shouldAnimateGif
        ? toMediaSrc(asset.path)
        : thumbPath
          ? toMediaSrc(thumbPath)
          : TRANSPARENT_THUMBNAIL_SRC
      : thumbPath
        ? toMediaSrc(thumbPath)
        : TRANSPARENT_THUMBNAIL_SRC;

  return (
    <>
      <ThumbnailImage
        className="block h-full w-full object-cover"
        src={src}
        alt={asset.path}
      />
      {showRenderLoader ? (
        <div
          className="absolute inset-0 grid place-items-center bg-[radial-gradient(circle_at_center,oklch(var(--b2)/0.2),oklch(var(--b1)/0.62))]"
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
  itemKey: string | number | bigint;
  itemIndex: number;
  itemLane: number;
  itemStart: number;
  tilePixelSize: number;
  tileGap: number;
  asset: Asset;
  thumbPath?: string;
  shouldAnimateGif: boolean;
  showRenderLoader: boolean;
  isBulkSelected: boolean;
  isLightboxSelected: boolean;
  groupedDescription: string;
  videoChipLabel: string;
  gifChipLabel: string;
  thumbnailStore?: ThumbnailStore;
  onMouseDown: (event: MouseEvent<HTMLButtonElement>) => void;
  onMouseEnter: (event: MouseEvent<HTMLButtonElement>) => void;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}

export const GalleryTile = memo(function GalleryTile({
  itemKey,
  itemIndex,
  itemLane,
  itemStart,
  tilePixelSize,
  tileGap,
  asset,
  thumbPath,
  shouldAnimateGif,
  showRenderLoader,
  isBulkSelected,
  isLightboxSelected,
  groupedDescription,
  videoChipLabel,
  gifChipLabel,
  thumbnailStore,
  onMouseDown,
  onMouseEnter,
  onClick
}: GalleryTileProps) {
  const isGrouped = Boolean(asset.media_group_key?.trim());
  useSyncExternalStore(
    thumbnailStore
      ? (listener) => thumbnailStore.subscribe(asset.id, listener)
      : () => () => {},
    () => thumbnailStore?.getVersion(asset.id) ?? 0,
    () => 0
  );
  const effectiveThumbPath = thumbnailStore?.getPath(asset.id) ?? thumbPath;
  const effectiveRenderLoader = thumbnailStore
    ? thumbnailStore.isRendering(asset.id) && !effectiveThumbPath
    : showRenderLoader;

  return (
    <button
      key={itemKey}
      className={`absolute z-[1] overflow-hidden rounded-[var(--radius-surface)] border border-white/8 bg-base-300/95 p-0 ${
        isBulkSelected
          ? "shadow-[0_0_0_3px_var(--color-warning),var(--shadow-tile-hover)]"
          : isLightboxSelected
            ? "shadow-[0_0_0_3px_oklch(var(--p)/0.72),var(--shadow-tile-hover)]"
            : "shadow-[var(--shadow-tile)] hover:shadow-[var(--shadow-tile-hover)]"
      }`}
      style={{
        width: tilePixelSize,
        height: tilePixelSize,
        transform: `translate3d(${itemLane * (tilePixelSize + tileGap)}px, ${itemStart}px, 0)`
      }}
      data-asset-id={asset.id}
      data-asset-index={itemIndex}
      onMouseDown={onMouseDown}
      onMouseEnter={onMouseEnter}
      onClick={onClick}
      aria-pressed={isBulkSelected || isLightboxSelected}
    >
      <TilePreview
        asset={asset}
        thumbPath={effectiveThumbPath}
        shouldAnimateGif={shouldAnimateGif}
        showRenderLoader={effectiveRenderLoader}
      />
      {isGrouped ? <span className="sr-only">{groupedDescription}</span> : null}
      {asset.kind === "video" || asset.kind === "gif" ? (
        <span
          className={`pointer-events-none absolute inset-0 rounded-[var(--radius-surface)] border-2 ${
            asset.kind === "video" ? "border-accent" : "border-info"
          }`}
          aria-hidden="true"
        />
      ) : null}
      {asset.kind === "video" ? (
        <UiChip className="pointer-events-none absolute right-0 top-0 shadow-[var(--shadow-chip)]" tone="video">
          {videoChipLabel}
        </UiChip>
      ) : null}
      {asset.kind === "gif" ? (
        <UiChip className="pointer-events-none absolute right-0 top-0 shadow-[var(--shadow-chip)]" tone="gif">
          {gifChipLabel}
        </UiChip>
      ) : null}
    </button>
  );
});
