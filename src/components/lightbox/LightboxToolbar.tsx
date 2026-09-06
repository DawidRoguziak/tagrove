import { UiIconButton } from "../UI/UiIconButton";
import { LightboxInfoPanel } from "./LightboxInfoPanel";
import { LightboxTagPanel } from "./LightboxTagPanel";
import { LightboxDeleteConfirmDialog } from "./LightboxDeleteConfirmDialog";
import { MediaGroupSetter } from "./MediaGroupSetter";
import type { SelectedAsset } from "../../types";
import { useEffect, useRef, type RefObject } from "react";

const DELETE_BUTTON_ID = "lightbox-delete-button";
export const SIDEBAR_CLOSE_BUTTON_ID = "lightbox-sidebar-close-button";

interface LightboxToolbarProps {
  selected: SelectedAsset;
  mediaGroupKeyEditor: string;
  mediaGroupOrderEditor: string;
  groupCopyConfirmed: boolean;
  canCopyMediaGroup: boolean;
  copyMediaGroupTitle: string;
  isNarrow: boolean;
  sidebarOpen: boolean;
  sidebarVisible: boolean;
  isFullscreen: boolean;
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: RefObject<HTMLInputElement | null>;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagEditingDisabled: boolean;
  onRemoveTag: (tag: string) => void;
  onTagDraftChange: (value: string) => void;
  onAddTag: (value: string) => void;
  onRetryTags: () => void;
  onRetryTagDetails: () => void;
  onMediaGroupKeyChange: (value: string) => void;
  onMediaGroupOrderChange: (value: string) => void;
  onApplyMediaGroup: () => void;
  infoPanelOpen: boolean;
  onToggleInfo: () => void;
  onCloseSidebar: (restoreFocus: boolean) => void;
  favoritePending?: boolean;
  groupPending?: boolean;
  onToggleFavorite: () => void;
  onCopyMediaGroup: () => void;
  onResetZoom: () => void;
  onToggleFullscreen: () => void;
  onOpenDeleteConfirm: () => void;
  onCloseLightbox: () => void;
  deleteConfirmOpen: boolean;
  deleteSubmitting: boolean;
  deleteError?: string | null;
  onDeleteConfirmClose: () => void;
  onDeleteConfirmSubmit: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

export function LightboxToolbar({
  selected,
  mediaGroupKeyEditor,
  mediaGroupOrderEditor,
  groupCopyConfirmed,
  canCopyMediaGroup,
  copyMediaGroupTitle,
  isNarrow,
  sidebarOpen,
  sidebarVisible,
  isFullscreen,
  selectedTags,
  tagDraft,
  knownTags,
  tagInputRef,
  tagSaving,
  tagFailed,
  tagDetailsLoading,
  tagDetailsFailed,
  tagEditingDisabled,
  onRemoveTag,
  onTagDraftChange,
  onAddTag,
  onRetryTags,
  onRetryTagDetails,
  onMediaGroupKeyChange,
  onMediaGroupOrderChange,
  onApplyMediaGroup,
  infoPanelOpen,
  onToggleInfo,
  onCloseSidebar,
  favoritePending = false,
  groupPending = false,
  onToggleFavorite,
  onCopyMediaGroup,
  onResetZoom,
  onToggleFullscreen,
  onOpenDeleteConfirm,
  onCloseLightbox,
  deleteConfirmOpen,
  deleteSubmitting,
  deleteError = null,
  onDeleteConfirmClose,
  onDeleteConfirmSubmit,
  t
}: LightboxToolbarProps) {
  const sidebarRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    sidebarRef.current?.toggleAttribute("inert", !sidebarVisible);
  }, [sidebarVisible]);

  return (
    <aside
      ref={sidebarRef}
      id="lightbox-sidebar"
      className={[
        "grid h-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)_auto] gap-3 overflow-hidden border-l border-[var(--border-soft)] bg-[var(--surface-solid)] p-3",
        "absolute inset-y-0 right-0 z-[8] transition-transform duration-200 motion-reduce:transition-none",
        isNarrow ? "w-[min(22rem,100%)]" : "w-[clamp(18rem,22vw,22rem)]",
        sidebarVisible ? "translate-x-0" : "pointer-events-none translate-x-full"
      ].join(" ")}
      data-lightbox-toolbar={selected.kind}
      aria-label={t("lightbox.sidebarAria")}
      aria-hidden={!sidebarVisible}
    >
      <header className="flex min-h-0 min-w-0 items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-3">
        <span className="min-w-0 truncate text-xs font-semibold text-base-content" title={selected.file_name}>
          {selected.file_name}
        </span>
        <div className="flex shrink-0 items-center gap-1">
          <UiIconButton
            id={SIDEBAR_CLOSE_BUTTON_ID}
            icon="arrow-left"
            iconClassName="h-3.5 w-3.5 shrink-0 rotate-180"
            className="h-8! w-8! min-h-8!"
            aria-label={t("lightbox.closePanel")}
            aria-expanded={sidebarOpen}
            aria-controls="lightbox-sidebar"
            title={t("lightbox.closePanel")}
            disabled={deleteConfirmOpen}
            onClick={(event) => onCloseSidebar(event.detail === 0)}
          />
          <UiIconButton
            icon="close"
            iconClassName="h-3.5 w-3.5 shrink-0"
            className="h-8! w-8! min-h-8!"
            aria-label={t("lightbox.closePreview")}
            title={t("common.close")}
            disabled={deleteSubmitting}
            onClick={onCloseLightbox}
          />
        </div>
      </header>

      <div
        className="panel-scroll flex min-h-0 min-w-0 flex-col gap-4 overflow-x-hidden overflow-y-auto pr-0.5"
        data-testid="lightbox-sidebar-upper"
      >
        {infoPanelOpen ? <LightboxInfoPanel selected={selected} /> : null}

        <section
          aria-label={t("lightbox.mediaGroup")}
          data-testid="lightbox-media-group-panel"
          className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5"
        >
          <h3 className="m-0 text-xs">{t("lightbox.mediaGroup")}</h3>
          <MediaGroupSetter
            groupKey={mediaGroupKeyEditor}
            groupOrder={mediaGroupOrderEditor}
            onGroupKeyChange={onMediaGroupKeyChange}
            onGroupOrderChange={onMediaGroupOrderChange}
            onApply={onApplyMediaGroup}
            pending={groupPending}
          />
        </section>

        <LightboxTagPanel
          selectedTags={selectedTags}
          tagDraft={tagDraft}
          knownTags={knownTags}
          tagInputRef={tagInputRef}
          tagSaving={tagSaving}
          tagFailed={tagFailed}
          tagDetailsLoading={tagDetailsLoading}
          tagDetailsFailed={tagDetailsFailed}
          tagEditingDisabled={tagEditingDisabled}
          onRemoveTag={onRemoveTag}
          onTagDraftChange={onTagDraftChange}
          onAddTag={onAddTag}
          onRetryTags={onRetryTags}
          onRetryTagDetails={onRetryTagDetails}
        />
      </div>

      <div className={`grid max-h-[calc(100dvh-100px)] min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5 ${deleteConfirmOpen ? "panel-scroll overflow-y-auto" : "overflow-visible"}`}>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(36px,1fr))] items-center gap-1 border-t border-[var(--border-soft)] pt-3" data-testid="lightbox-action-rail">
          <UiIconButton
            icon="heart"
            iconClassName="h-4 w-4 shrink-0"
            className="h-8! w-8! min-h-8! justify-self-center"
            active={selected.is_favorite}
            aria-label={selected.is_favorite ? t("lightbox.favorite.remove") : t("lightbox.favorite.add")}
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
          <UiIconButton
            icon="info"
            iconClassName="h-4 w-4 shrink-0"
            className="h-8! w-8! min-h-8! justify-self-center"
            active={infoPanelOpen}
            aria-expanded={infoPanelOpen}
            aria-label={t("lightbox.showInfo")}
            title={infoPanelOpen ? t("lightbox.hideInfo") : t("lightbox.showInfo")}
            onClick={onToggleInfo}
          />
          {selected.kind === "video" ? null : (
            <UiIconButton
              icon="fullscreen"
              iconClassName="h-4 w-4 shrink-0"
              className="h-8! w-8! min-h-8! justify-self-center"
              active={isFullscreen}
              aria-label={t("lightbox.toggleFullscreen")}
              title={isFullscreen ? t("lightbox.exitFullscreen") : t("lightbox.fullscreen")}
              onClick={onToggleFullscreen}
            />
          )}
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

        <LightboxDeleteConfirmDialog
          open={deleteConfirmOpen}
          isSubmitting={deleteSubmitting}
          errorMessage={deleteError}
          onClose={onDeleteConfirmClose}
          onConfirm={onDeleteConfirmSubmit}
        />
      </div>
    </aside>
  );
}
