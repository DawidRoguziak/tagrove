import type { SelectedAsset } from "../../types";
import { formatBytes } from "../../utils/media";
import { UiIconButton } from "../UI/UiIconButton";

const DELETE_BUTTON_ID = "lightbox-delete-button";

interface LightboxActionRailProps {
  visible: boolean;
  selected: SelectedAsset;
  groupCopyConfirmed: boolean;
  canCopyMediaGroup: boolean;
  copyMediaGroupTitle: string;
  isFullscreen: boolean;
  favoritePending?: boolean;
  onToggleFavorite: () => void;
  onCopyMediaGroup: () => void;
  onResetZoom: () => void;
  onToggleFullscreen: () => void;
  onOpenDeleteConfirm: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

export function LightboxActionRail({
  visible,
  selected,
  groupCopyConfirmed,
  canCopyMediaGroup,
  copyMediaGroupTitle,
  isFullscreen,
  favoritePending,
  onToggleFavorite,
  onCopyMediaGroup,
  onResetZoom,
  onToggleFullscreen,
  onOpenDeleteConfirm,
  t
}: LightboxActionRailProps) {
  return (
    <div
      className="lightbox-action-row transition-opacity duration-200 motion-reduce:transition-none"
      style={{ opacity: visible ? 1 : 0, pointerEvents: visible ? undefined : "none" }}
    >
      <div className="lightbox-action-island" data-testid="lightbox-action-rail">
        <UiIconButton
          icon="heart"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={selected.is_favorite}
          aria-label={
            selected.is_favorite ? t("lightbox.favorite.remove") : t("lightbox.favorite.add")
          }
          title={selected.is_favorite ? t("lightbox.favorite.on") : t("lightbox.favorite.off")}
          onClick={onToggleFavorite}
          disabled={favoritePending}
          aria-busy={favoritePending}
        />
        <UiIconButton
          icon={groupCopyConfirmed ? "check-square" : "copy"}
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={groupCopyConfirmed}
          disabled={!canCopyMediaGroup}
          aria-label={copyMediaGroupTitle}
          title={copyMediaGroupTitle}
          onClick={onCopyMediaGroup}
        />
        <UiIconButton
          icon="reset"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          disabled={selected.kind === "video"}
          aria-label={t("lightbox.resetZoom")}
          title={t("lightbox.reset")}
          onClick={onResetZoom}
        />
        <span className="lightbox-media-summary">
          {selected.width !== null && selected.height !== null ? (
            <span>
              {selected.width}×{selected.height}
            </span>
          ) : null}
          {selected.size_bytes !== null ? <span>{formatBytes(selected.size_bytes)}</span> : null}
          <span>{selected.kind.toUpperCase()}</span>
        </span>
        <UiIconButton
          icon="fullscreen"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={isFullscreen}
          aria-label={t("lightbox.toggleFullscreen")}
          title={isFullscreen ? t("lightbox.exitFullscreen") : t("lightbox.fullscreen")}
          onClick={onToggleFullscreen}
        />
        <UiIconButton
          id={DELETE_BUTTON_ID}
          icon="trash"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          danger
          aria-label={t("lightbox.deleteMedia")}
          title={t("lightbox.deleteMedia")}
          onClick={onOpenDeleteConfirm}
        />
      </div>
    </div>
  );
}
