import { useRef } from "react";
import type { Asset } from "../../types";
import { LightboxDeleteConfirmDialog } from "./LightboxDeleteConfirmDialog";
import { LightboxMediaStage } from "./LightboxMediaStage";
import { LightboxToolbar } from "./LightboxToolbar";
import { useLightboxModalHandlers } from "./hooks/useLightboxModalHandlers";
import { useLightboxMediaGroupClipboard } from "./hooks/useLightboxMediaGroupClipboard";
import { useLightboxImageControls } from "./useLightboxImageControls";
import { useLightboxTagging } from "./useLightboxTagging";
import { useTranslation } from "react-i18next";

interface LightboxModalProps {
  selected: Asset | null;
  tagEditor: string[];
  onTagEditorChange: (value: string[]) => void;
  onSaveTags: (tags?: string[]) => void | Promise<void>;
  onRetryTags?: () => void;
  tagSaving?: boolean;
  tagFailed?: boolean;
  tagDetailsLoading?: boolean;
  tagDetailsFailed?: boolean;
  onRetryTagDetails?: () => void;
  mediaGroupKeyEditor?: string;
  mediaGroupOrderEditor?: string;
  onMediaGroupKeyEditorChange?: (value: string) => void;
  onMediaGroupOrderEditorChange?: (value: string) => void;
  onSaveMediaGroup?: (next: { key: string | null; order: number | null }) => void | Promise<void>;
  knownTags: string[];
  onNavigatePrevious: () => void;
  onNavigateNext: () => void;
  onToggleFavorite: () => void;
  onDeleteMedia?: () => void | Promise<void>;
  onClose: () => void;
}

export function LightboxModal({
  selected,
  tagEditor,
  onTagEditorChange,
  onSaveTags,
  onRetryTags = () => {},
  tagSaving = false,
  tagFailed = false,
  tagDetailsLoading = false,
  tagDetailsFailed = false,
  onRetryTagDetails = () => {},
  mediaGroupKeyEditor = "",
  mediaGroupOrderEditor = "",
  onMediaGroupKeyEditorChange = () => {},
  onMediaGroupOrderEditorChange = () => {},
  onSaveMediaGroup,
  knownTags,
  onNavigatePrevious,
  onNavigateNext,
  onToggleFavorite,
  onDeleteMedia = () => {},
  onClose
}: LightboxModalProps) {
  const { t } = useTranslation();
  const tagPopoverContainerRef = useRef<HTMLDivElement | null>(null);
  const infoPopoverContainerRef = useRef<HTMLDivElement | null>(null);
  const selectedId = selected?.id ?? null;

  const handlers = useLightboxModalHandlers({
    selectedId,
    mediaGroupKeyEditor,
    mediaGroupOrderEditor,
    onSaveMediaGroup,
    onDeleteMedia,
    onClose,
    tagPopoverContainerRef,
    infoPopoverContainerRef
  });

  const tagging = useLightboxTagging({
    selectedId,
    tagEditor,
    onTagEditorChange,
    onSaveTags,
    onRetryTags,
    tagSaving,
    tagFailed,
    tagDetailsLoading,
    tagDetailsFailed,
    knownTags,
    tagsPanelOpen: handlers.tagsPanelOpen
  });

  const mediaControls = useLightboxImageControls({
    selected,
    onClose,
    onNavigatePrevious,
    onNavigateNext,
    onEnterFullscreen: handlers.handleEnterFullscreen
  });

  const clipboard = useLightboxMediaGroupClipboard(mediaGroupKeyEditor, selectedId);
  const groupCopyConfirmed = clipboard.groupCopyConfirmed;
  const copyMediaGroupTitle = groupCopyConfirmed
    ? t("lightbox.copyMediaGroup.copied")
    : clipboard.canCopyMediaGroup
      ? t("lightbox.copyMediaGroup.title")
      : t("lightbox.copyMediaGroup.unavailable");

  if (!selected) return null;

  return (
    <div
      className="fixed inset-0 z-[55] grid place-items-center bg-neutral/62 p-2 backdrop-blur-sm sm:p-5"
      onClick={mediaControls.tryCloseLightbox}
    >
      <div
        className={[
          "relative overflow-hidden border border-[var(--border-soft)] bg-[var(--surface-solid)] shadow-[var(--shadow-modal)] transition-all duration-200",
          "h-[min(96vh,1180px)] min-h-[360px] w-[min(99vw,1780px)] rounded-[var(--radius-surface)]",
          "sm:min-h-[460px] sm:w-[min(97vw,1680px)] sm:rounded-[var(--radius-panel)]",
          "lg:h-[min(95vh,1280px)] lg:w-[min(96vw,1840px)]",
          selected.kind !== "video" && mediaControls.isFullscreen
            ? "h-full min-h-0 w-full rounded-none border-0 bg-base-100 shadow-none"
            : ""
        ]
          .filter(Boolean)
          .join(" ")}
        ref={mediaControls.lightboxShellRef}
        onPointerDownCapture={handlers.handleShellPointerDownCapture}
        onClick={handlers.handleShellClick}
      >
        <LightboxToolbar
          selected={selected}
          mediaGroupKeyEditor={mediaGroupKeyEditor}
          mediaGroupOrderEditor={mediaGroupOrderEditor}
          groupCopyConfirmed={groupCopyConfirmed}
          canCopyMediaGroup={clipboard.canCopyMediaGroup}
          copyMediaGroupTitle={copyMediaGroupTitle}
          isFullscreen={mediaControls.isFullscreen}
          tagsPanelOpen={handlers.tagsPanelOpen}
          infoPanelOpen={handlers.infoPanelOpen}
          selectedTags={tagging.selectedTags}
          tagDraft={tagging.tagDraft}
          knownTags={tagging.availableKnownTags}
          tagInputRef={tagging.tagInputRef}
          tagSaving={tagging.tagSaving}
          tagFailed={tagging.tagFailed}
          tagDetailsLoading={tagDetailsLoading}
          tagDetailsFailed={tagDetailsFailed}
          tagEditingDisabled={tagging.tagEditingDisabled}
          tagPopoverContainerRef={tagPopoverContainerRef}
          infoPopoverContainerRef={infoPopoverContainerRef}
          onToggleTagsPanel={handlers.handleToggleTagsPanel}
          onRemoveTag={tagging.removeTag}
          onTagDraftChange={tagging.setTagDraft}
          onAddTag={tagging.addTag}
          onRetryTags={tagging.retryTags}
          onRetryTagDetails={onRetryTagDetails}
          onMediaGroupKeyChange={onMediaGroupKeyEditorChange}
          onMediaGroupOrderChange={onMediaGroupOrderEditorChange}
          onApplyMediaGroup={handlers.handleApplyMediaGroup}
          onToggleInfoPanel={handlers.handleToggleInfoPanel}
          onToggleFavorite={onToggleFavorite}
          onCopyMediaGroup={() => {
            void clipboard.copyMediaGroup();
          }}
          onResetZoom={mediaControls.resetZoom}
          onToggleFullscreen={() => {
            void mediaControls.toggleFullscreen();
          }}
          onOpenDeleteConfirm={handlers.handleOpenDeleteConfirm}
          onCloseLightbox={handlers.handleCloseLightbox}
          t={t}
        />

        <LightboxMediaStage
          selected={selected}
          mediaViewportRef={mediaControls.mediaViewportRef}
          lightboxImageRef={mediaControls.lightboxImageRef}
          lightboxVideoPlayerRef={mediaControls.lightboxVideoPlayerRef}
          isZoomed={mediaControls.isZoomed}
          mediaDisplaySize={mediaControls.mediaDisplaySize}
          isDragging={mediaControls.isDragging}
          isFullscreen={mediaControls.isFullscreen}
          onImageLoad={mediaControls.handleImageLoad}
          onVideoLoadedMetadata={mediaControls.handleVideoLoadedMetadata}
          onVideoFullscreenChange={mediaControls.handleVideoFullscreenChange}
          onImageClick={mediaControls.handleImageClick}
          onImagePointerDown={mediaControls.handleImagePointerDown}
          onImagePointerMove={mediaControls.handleImagePointerMove}
          onImagePointerEnd={mediaControls.handleImagePointerEnd}
        />
      </div>

      <LightboxDeleteConfirmDialog
        open={handlers.deleteConfirmOpen}
        isSubmitting={handlers.deleteSubmitting}
        onClose={handlers.handleCloseDeleteConfirm}
        onConfirm={handlers.handleConfirmDeleteMedia}
      />
    </div>
  );
}
