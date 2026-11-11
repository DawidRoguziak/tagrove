import { createSignal, createEffect } from "solid-js";
import type { SelectedAsset } from "./types";
import { formatBytes } from "../../utils/media";
import { UiIconButton } from "../../ui/controls/UiIconButton";

const DELETE_BUTTON_ID = "lightbox-delete-button";

interface LightboxActionRailProps {
  visible: boolean;
  covered: boolean;
  onReveal: () => void;
  selected: SelectedAsset;
  isFullscreen: boolean;
  favoritePending?: boolean;
  onToggleFavorite: () => void;
  onResetZoom: () => void;
  onToggleFullscreen: () => void;
  onOpenDeleteConfirm: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

export function LightboxActionRail(props: LightboxActionRailProps) {
  const [hovered, setHovered] = createSignal(false);
  createEffect(
    () => props.covered,
    (covered) => {
      if (covered) setHovered(false);
    }
  );
  const controlsVisible = () => props.visible || (hovered() && !props.covered);
  const endHover = () => {
    props.onReveal();
    setHovered(false);
  };

  return (
    <div
      class="lightbox-action-row transition-opacity duration-200 motion-reduce:transition-none"
      style={{
        opacity: controlsVisible() ? 1 : 0,
        "pointer-events": controlsVisible() ? undefined : "none"
      }}
    >
      <div
        class="lightbox-action-island"
        data-testid="lightbox-action-rail"
        onPointerEnter={(event) => {
          if (!props.covered && event.pointerType !== "touch") setHovered(true);
        }}
        onPointerLeave={endHover}
        onPointerCancel={endHover}
      >
        <UiIconButton
          icon="heart"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={props.selected.is_favorite}
          aria-label={
            props.selected.is_favorite
              ? props.t("lightbox.favorite.remove")
              : props.t("lightbox.favorite.add")
          }
          title={
            props.selected.is_favorite
              ? props.t("lightbox.favorite.on")
              : props.t("lightbox.favorite.off")
          }
          onClick={props.onToggleFavorite}
          disabled={props.favoritePending}
          aria-busy={props.favoritePending ? "true" : "false"}
        />
        <UiIconButton
          icon="reset"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          disabled={props.selected.kind === "video"}
          aria-label={props.t("lightbox.resetZoom")}
          title={props.t("lightbox.reset")}
          onClick={props.onResetZoom}
        />
        <span class="lightbox-media-summary">
          {props.selected.width !== null && props.selected.height !== null ? (
            <span>
              {props.selected.width}×{props.selected.height}
            </span>
          ) : null}
          {props.selected.size_bytes !== null ? (
            <span>{formatBytes(props.selected.size_bytes)}</span>
          ) : null}
          <span>{props.t(`lightbox.kind.${props.selected.kind}`).toUpperCase()}</span>
        </span>
        <UiIconButton
          icon="fullscreen"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={props.isFullscreen}
          aria-label={props.t("lightbox.toggleFullscreen")}
          title={
            props.isFullscreen ? props.t("lightbox.exitFullscreen") : props.t("lightbox.fullscreen")
          }
          onClick={props.onToggleFullscreen}
        />
        <UiIconButton
          id={DELETE_BUTTON_ID}
          icon="trash"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          danger
          aria-label={props.t("lightbox.deleteMedia")}
          title={props.t("lightbox.deleteMedia")}
          onClick={props.onOpenDeleteConfirm}
        />
      </div>
    </div>
  );
}
