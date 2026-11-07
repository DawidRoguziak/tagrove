import { createMemo, createSignal } from "solid-js";
import { thumbnailUrl } from "../../platform/tauri/thumbnails";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import { ThumbnailImage } from "../../ui/controls/ThumbnailImage";
import { UiIcon } from "../../ui/controls/UiIcon";
import { UiChip } from "../../ui/controls/UiChip";
import { useLanguage } from "../../i18n/solid";
/** One grid cell; the row's CSS grid places and sizes it. */
export function GalleryTile(props: {
  asset: AssetSummary;
  index: number;
  selected: boolean;
  /** Fast-scroll pause: newly shown assets must not start thumbnail renders. */
  thumbnailsPaused: boolean;
  /** Overscan tile: its render waits behind every visible tile's. */
  prefetch: boolean;
  onClick: (event: MouseEvent & { currentTarget: HTMLButtonElement }) => void;
}) {
  const { t } = useLanguage();
  // The backend serves or renders the thumbnail for this URL (`thumb://`). An asset
  // first shown during a fast-scroll pause asks for a cached file only, and one first
  // shown in the overscan asks at prefetch priority. Both choices stick for the asset,
  // so its URL never changes while it stays in this tile; only a cached-only miss
  // switches to the rendering URL once the pause ends.
  const key = () => `${props.asset.id}:${props.asset.thumb_version}`;
  const shownAs = createMemo<{ key: string; cachedOnly: boolean; prefetch: boolean }>(
    (previous) =>
      previous?.key === key()
        ? previous
        : { key: key(), cachedOnly: props.thumbnailsPaused, prefetch: props.prefetch }
  );
  const [missed, setMissed] = createSignal<string | null>(null);
  const cachedOnly = () =>
    shownAs().cachedOnly && (props.thumbnailsPaused || missed() !== shownAs().key);
  const src = () =>
    thumbnailUrl(props.asset.id, props.asset.thumb_version, {
      cachedOnly: cachedOnly(),
      prefetch: shownAs().prefetch
    });
  const retrySrc = (retry: number) =>
    thumbnailUrl(props.asset.id, props.asset.thumb_version, {
      retry,
      prefetch: shownAs().prefetch
    });
  return (
    <button
      class={`relative z-[1] h-full w-full overflow-hidden rounded-[var(--radius-tile)] border border-[var(--border-soft)] bg-base-300/95 p-0 ${props.selected ? "shadow-[0_0_0_2px_var(--color-primary),var(--shadow-tile-hover)]" : "shadow-[var(--shadow-tile)] hover:shadow-[var(--shadow-tile-hover)]"}`}
      data-asset-id={props.asset.id}
      data-asset-index={props.index}
      draggable={false}
      onClick={props.onClick}
      aria-pressed={props.selected ? "true" : "false"}
    >
      <ThumbnailImage
        className="block h-full w-full object-cover"
        src={src()}
        retrySrc={cachedOnly() ? undefined : retrySrc}
        onFailed={() => {
          if (cachedOnly()) setMissed(shownAs().key);
        }}
        alt={props.asset.preview_path ?? props.asset.file_name}
      />
      {/* Shown by CSS only while the image above is still `invisible`, after a short
          delay, so cached thumbnails never flash a spinner. */}
      <div
        class="thumbnail-loader absolute inset-0 grid place-items-center bg-base-100/60"
        aria-hidden="true"
        data-testid="tile-thumb-loader"
      >
        <span class="h-[26px] w-[26px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
      </div>
      {props.asset.media_group_key?.trim() ? (
        <span class="sr-only">{t("gallery.groupedChip")}</span>
      ) : null}
      {props.selected ? (
        <span
          class="gallery-selection-mark pointer-events-none absolute left-2 top-2 grid h-5 w-5 place-items-center rounded-[var(--radius-tile)] bg-primary text-primary-content"
          aria-hidden="true"
        >
          <UiIcon name="check-square" className="h-4 w-4" />
        </span>
      ) : null}
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
