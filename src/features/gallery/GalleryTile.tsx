import { untrack } from "solid-js";
import { toMediaSrc } from "../../platform/tauri/assets";
import { createAssetThumbnail } from "../../library/thumbnails/createAssetThumbnail";
import type { ThumbnailStore } from "../../library/thumbnails/thumbnailStore";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../../ui/controls/ThumbnailImage";
import { UiIcon } from "../../ui/controls/UiIcon";
import { UiChip } from "../../ui/controls/UiChip";
import { useLanguage } from "../../i18n/solid";
export function GalleryTile(props: {
  asset: AssetSummary;
  index: number;
  lane: number;
  start: number;
  size: number;
  gap: number;
  selected: boolean;
  thumbnailStore: ThumbnailStore;
  onClick: (event: MouseEvent & { currentTarget: HTMLButtonElement }) => void;
}) {
  const { t } = useLanguage();
  const thumbnail = createAssetThumbnail(
    untrack(() => props.thumbnailStore),
    () => props.asset.id
  );
  const src = () => {
    const path = thumbnail().path;
    return path ? toMediaSrc(path) : TRANSPARENT_THUMBNAIL_SRC;
  };
  return (
    <button
      class={`absolute z-[1] overflow-hidden rounded-[var(--radius-tile)] border border-[var(--border-soft)] bg-base-300/95 p-0 ${props.selected ? "shadow-[0_0_0_2px_var(--color-primary),var(--shadow-tile-hover)]" : "shadow-[var(--shadow-tile)] hover:shadow-[var(--shadow-tile-hover)]"}`}
      style={{
        width: `${props.size}px`,
        height: `${props.size}px`,
        transform: `translate3d(${props.lane * (props.size + props.gap)}px, ${props.start}px, 0)`
      }}
      data-asset-id={props.asset.id}
      data-asset-index={props.index}
      draggable={false}
      onClick={props.onClick}
      aria-pressed={props.selected ? "true" : "false"}
    >
      <ThumbnailImage
        fadeIn
        className="block h-full w-full object-cover"
        src={src()}
        alt={props.asset.preview_path ?? props.asset.file_name}
      />
      {thumbnail().rendering && !thumbnail().path ? (
        <div
          class="absolute inset-0 grid place-items-center bg-base-100/60"
          aria-hidden="true"
          data-testid="tile-thumb-loader"
        >
          <span class="h-[26px] w-[26px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
        </div>
      ) : null}
      {props.asset.media_group_key?.trim() ? (
        <span class="sr-only">{t("gallery.groupedChip")}</span>
      ) : null}
      <span
        class="gallery-selection-mark pointer-events-none absolute left-2 top-2 grid h-5 w-5 place-items-center rounded-[var(--radius-tile)] bg-primary text-primary-content"
        style={{ opacity: props.selected ? 1 : 0 }}
        aria-hidden="true"
      >
        <UiIcon name="check-square" className="h-4 w-4" />
      </span>
      {props.asset.kind === "video" ? (
        <UiChip
          className="pointer-events-none absolute right-1 bottom-1 shadow-[var(--shadow-chip)]"
          tone="video"
        >
          {t("gallery.videoChip")}
        </UiChip>
      ) : null}
      {props.asset.kind === "gif" ? (
        <UiChip
          className="pointer-events-none absolute right-1 bottom-1 shadow-[var(--shadow-chip)]"
          tone="gif"
        >
          {t("gallery.gifChip")}
        </UiChip>
      ) : null}
    </button>
  );
}
