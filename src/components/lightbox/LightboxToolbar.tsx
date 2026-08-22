import { UiIconButton } from "../UI/UiIconButton";
import { LightboxInfoPanel } from "./LightboxInfoPanel";
import { LightboxTagPanel } from "./LightboxTagPanel";
import type { SelectedAsset } from "../../types";
import type { MutableRefObject, RefObject } from "react";

interface LightboxToolbarProps {
  selected: SelectedAsset;
  mediaGroupKeyEditor: string;
  mediaGroupOrderEditor: string;
  groupCopyConfirmed: boolean;
  canCopyMediaGroup: boolean;
  copyMediaGroupTitle: string;
  isFullscreen: boolean;
  tagsPanelOpen: boolean;
  infoPanelOpen: boolean;
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: RefObject<HTMLInputElement | null>;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagEditingDisabled: boolean;
  tagPopoverContainerRef: MutableRefObject<HTMLDivElement | null>;
  infoPopoverContainerRef: MutableRefObject<HTMLDivElement | null>;
  onToggleTagsPanel: () => void;
  onRemoveTag: (tag: string) => void;
  onTagDraftChange: (value: string) => void;
  onAddTag: (value: string) => void;
  onRetryTags: () => void;
  onRetryTagDetails: () => void;
  onMediaGroupKeyChange: (value: string) => void;
  onMediaGroupOrderChange: (value: string) => void;
  onApplyMediaGroup: () => void;
  onToggleInfoPanel: () => void;
  onToggleFavorite: () => void;
  onCopyMediaGroup: () => void;
  onResetZoom: () => void;
  onToggleFullscreen: () => void;
  onOpenDeleteConfirm: () => void;
  onCloseLightbox: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

export function LightboxToolbar({
  selected,
  mediaGroupKeyEditor,
  mediaGroupOrderEditor,
  groupCopyConfirmed,
  canCopyMediaGroup,
  copyMediaGroupTitle,
  isFullscreen,
  tagsPanelOpen,
  infoPanelOpen,
  selectedTags,
  tagDraft,
  knownTags,
  tagInputRef,
  tagSaving,
  tagFailed,
  tagDetailsLoading,
  tagDetailsFailed,
  tagEditingDisabled,
  tagPopoverContainerRef,
  infoPopoverContainerRef,
  onToggleTagsPanel,
  onRemoveTag,
  onTagDraftChange,
  onAddTag,
  onRetryTags,
  onRetryTagDetails,
  onMediaGroupKeyChange,
  onMediaGroupOrderChange,
  onApplyMediaGroup,
  onToggleInfoPanel,
  onToggleFavorite,
  onCopyMediaGroup,
  onResetZoom,
  onToggleFullscreen,
  onOpenDeleteConfirm,
  onCloseLightbox,
  t
}: LightboxToolbarProps) {
  return (
    <div
      className={`absolute right-2.5 z-[4] flex gap-1.5 rounded-2xl border border-white/10 bg-neutral/34 p-1.5 shadow-[var(--shadow-floating)] backdrop-blur-xl lg:bottom-auto lg:right-3.5 lg:top-1/2 lg:grid lg:-translate-y-1/2 ${
        selected.kind === "video" ? "bottom-20" : "bottom-2.5"
      }`}
    >
      <div
        ref={(node) => {
          tagPopoverContainerRef.current = node;
        }}
      >
        <LightboxTagPanel
          open={tagsPanelOpen}
          selectedTags={selectedTags}
          tagDraft={tagDraft}
          knownTags={knownTags}
          tagInputRef={tagInputRef}
          tagSaving={tagSaving}
          tagFailed={tagFailed}
          tagDetailsLoading={tagDetailsLoading}
          tagDetailsFailed={tagDetailsFailed}
          tagEditingDisabled={tagEditingDisabled}
          mediaGroupKey={mediaGroupKeyEditor}
          mediaGroupOrder={mediaGroupOrderEditor}
          onToggle={onToggleTagsPanel}
          onRemoveTag={onRemoveTag}
          onTagDraftChange={onTagDraftChange}
          onAddTag={onAddTag}
          onRetryTags={onRetryTags}
          onRetryTagDetails={onRetryTagDetails}
          onMediaGroupKeyChange={onMediaGroupKeyChange}
          onMediaGroupOrderChange={onMediaGroupOrderChange}
          onApplyMediaGroup={onApplyMediaGroup}
        />
      </div>

      <div
        ref={(node) => {
          infoPopoverContainerRef.current = node;
        }}
      >
        <LightboxInfoPanel open={infoPanelOpen} selected={selected} onToggle={onToggleInfoPanel} />
      </div>

      <UiIconButton
        icon="heart"
        active={selected.is_favorite}
        aria-label={selected.is_favorite ? t("lightbox.favorite.remove") : t("lightbox.favorite.add")}
        title={selected.is_favorite ? t("lightbox.favorite.on") : t("lightbox.favorite.off")}
        onClick={onToggleFavorite}
      />
      <UiIconButton
        icon={groupCopyConfirmed ? "check-square" : "copy"}
        active={groupCopyConfirmed}
        disabled={!canCopyMediaGroup}
        aria-label={copyMediaGroupTitle}
        title={copyMediaGroupTitle}
        onClick={onCopyMediaGroup}
      />
      <UiIconButton
        icon="reset"
        disabled={selected.kind === "video"}
        aria-label={t("lightbox.resetZoom")}
        title={t("lightbox.reset")}
        onClick={onResetZoom}
      />
      {selected.kind === "video" ? null : (
        <UiIconButton
          icon="fullscreen"
          active={isFullscreen}
          aria-label={t("lightbox.toggleFullscreen")}
          title={isFullscreen ? t("lightbox.exitFullscreen") : t("lightbox.fullscreen")}
          onClick={onToggleFullscreen}
        />
      )}
      <UiIconButton
        icon="trash"
        danger
        aria-label={t("lightbox.deleteMedia")}
        title={t("lightbox.deleteMedia")}
        onClick={onOpenDeleteConfirm}
      />
      <UiIconButton
        icon="close"
        aria-label={t("lightbox.closePreview")}
        title={t("common.close")}
        onClick={onCloseLightbox}
      />
    </div>
  );
}
