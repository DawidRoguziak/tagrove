import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { SelectedAsset } from "../../types";
import { LightboxDeleteConfirmDialog } from "./LightboxDeleteConfirmDialog";
import { LightboxMediaStage } from "./LightboxMediaStage";
import { LightboxToolbar } from "./LightboxToolbar";
import { useLightboxModalHandlers } from "./hooks/useLightboxModalHandlers";
import { useLightboxMediaGroupClipboard } from "./hooks/useLightboxMediaGroupClipboard";
import { useLightboxImageControls } from "./useLightboxImageControls";
import { useLightboxTagging } from "./useLightboxTagging";
import { useTranslation } from "react-i18next";
import { useUiLayer } from "../UI/UiLayerProvider";
import { UiAlert } from "../UI/UiAlert";

interface LightboxModalProps {
  selected: SelectedAsset | null;
  tagEditor: string[];
  onTagEditorChange: (value: string[]) => void;
  onSaveTags: (tags?: string[]) => void | Promise<void>;
  onRetryTags?: () => void;
  tagSaving?: boolean;
  tagFailed?: boolean;
  tagDetailsLoading?: boolean;
  tagDetailsFailed?: boolean;
  assetDetailsFailed?: boolean;
  onRetryTagDetails?: () => void;
  mediaGroupKeyEditor?: string;
  mediaGroupOrderEditor?: string;
  onMediaGroupKeyEditorChange?: (value: string) => void;
  onMediaGroupOrderEditorChange?: (value: string) => void;
  onSaveMediaGroup?: (next: { key: string | null; order: number | null }) => void | Promise<void>;
  knownTags: string[];
  onNavigatePrevious: () => void;
  onNavigateNext: () => void;
  onToggleFavorite: () => void | Promise<void>;
  onDeleteMedia?: () => void | Promise<void>;
  onClose: () => void;
  getRestoreFocus?: () => HTMLElement | null;
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
  assetDetailsFailed = false,
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
  onClose,
  getRestoreFocus
}: LightboxModalProps) {
  const { t } = useTranslation();
  const tagPopoverContainerRef = useRef<HTMLDivElement | null>(null);
  const infoPopoverContainerRef = useRef<HTMLDivElement | null>(null);
  const selectedId = selected?.id ?? null;
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;
  const [favoriteFailed, setFavoriteFailed] = useState(false);

  useEffect(() => {
    setFavoriteFailed(false);
  }, [selectedId]);

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
    keyboardShortcutsEnabled: !handlers.deleteConfirmOpen,
    selected,
    onClose,
    onNavigatePrevious,
    onNavigateNext,
    onEnterFullscreen: handlers.handleEnterFullscreen
  });

  const clipboard = useLightboxMediaGroupClipboard(mediaGroupKeyEditor, selectedId);
  const { isTopLayer, layerId } = useUiLayer({
    active: selected !== null,
    modal: true,
    containerRef: mediaControls.lightboxShellRef,
    closeOnEscape: !mediaControls.isFullscreen,
    onEscape: () => {
      onClose();
    },
    getRestoreFocus
  });
  const groupCopyConfirmed = clipboard.groupCopyConfirmed;
  const copyMediaGroupTitle = groupCopyConfirmed
    ? t("lightbox.copyMediaGroup.copied")
    : clipboard.canCopyMediaGroup
      ? t("lightbox.copyMediaGroup.title")
      : t("lightbox.copyMediaGroup.unavailable");
  const isVideo = selected?.kind === "video";
  const videoFullscreen = isVideo && mediaControls.isFullscreen;
  const imageFullscreen = !isVideo && mediaControls.isFullscreen;

  if (!selected) return null;

  return createPortal(
    <div
      className={[
        "fixed inset-0 z-[55] grid place-items-center",
        videoFullscreen
          ? "bg-neutral p-0 backdrop-blur-none"
          : `bg-neutral/62 backdrop-blur-sm ${isVideo ? "p-5" : "p-2 sm:p-5"}`
      ].join(" ")}
      onClick={() => {
        if (isTopLayer) mediaControls.tryCloseLightbox();
      }}
      data-ui-layer={layerId}
      data-lightbox-backdrop
    >
      <div
        className={[
          "relative overflow-hidden transition-all duration-200",
          videoFullscreen || imageFullscreen
            ? `h-full min-h-0 w-full rounded-none border-0 shadow-none ${
                videoFullscreen ? "bg-neutral" : "bg-base-100"
              }`
            : isVideo
              ? [
                  "lightbox-shell--video h-[min(calc(100vh-40px),1180px)] min-h-0",
                  "w-[min(calc(100vw-40px),1780px)] rounded-[var(--radius-surface)]",
                  "border shadow-[var(--shadow-modal)] sm:rounded-[var(--radius-panel)]",
                  "lg:h-[min(calc(100vh-40px),1280px)] lg:w-[min(calc(100vw-40px),1840px)]"
                ].join(" ")
              : [
                  "h-[min(96vh,1180px)] min-h-[360px] w-[min(99vw,1780px)] rounded-[var(--radius-surface)]",
                  "border border-[var(--border-soft)] bg-[var(--surface-solid)] shadow-[var(--shadow-modal)]",
                  "sm:min-h-[460px] sm:w-[min(97vw,1680px)] sm:rounded-[var(--radius-panel)]",
                  "lg:h-[min(95vh,1280px)] lg:w-[min(96vw,1840px)]"
                ].join(" ")
        ]
          .filter(Boolean)
          .join(" ")}
        ref={mediaControls.lightboxShellRef}
        role="dialog"
        aria-modal="true"
        aria-label={t("lightbox.previewDialog", { name: selected.file_name })}
        aria-describedby="lightbox-dialog-description"
        data-lightbox-kind={selected.kind}
        tabIndex={-1}
        onPointerDownCapture={handlers.handleShellPointerDownCapture}
        onClick={handlers.handleShellClick}
      >
        <span id="lightbox-dialog-description" className="sr-only">
          {t("lightbox.previewDialogDescription")}
        </span>
        {videoFullscreen ? null : (
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
            onToggleFavorite={() => {
              const requestedAssetId = selectedId;
              setFavoriteFailed(false);
              void Promise.resolve(onToggleFavorite()).catch(() => {
                if (selectedIdRef.current === requestedAssetId) setFavoriteFailed(true);
              });
            }}
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
        )}

        {favoriteFailed || handlers.mediaGroupFailed ? (
          <div className="absolute left-3 top-3 z-[5] grid max-w-[min(28rem,calc(100%-6rem))] gap-2">
            {favoriteFailed ? (
              <UiAlert tone="error" title={t("lightbox.saveFailedTitle")} className="shadow-[var(--shadow-floating)]">
                {t("lightbox.favoriteSaveFailed")}
              </UiAlert>
            ) : null}
            {handlers.mediaGroupFailed ? (
              <UiAlert tone="error" title={t("lightbox.saveFailedTitle")} className="shadow-[var(--shadow-floating)]">
                {t("lightbox.mediaGroupSaveFailed")}
              </UiAlert>
            ) : null}
          </div>
        ) : null}

        <LightboxMediaStage
          selected={selected}
          detailsLoading={tagDetailsLoading}
          detailsFailed={assetDetailsFailed}
          onRetryDetails={onRetryTagDetails}
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
        errorMessage={handlers.deleteError}
        onClose={handlers.handleCloseDeleteConfirm}
        onConfirm={handlers.handleConfirmDeleteMedia}
      />
    </div>,
    document.body
  );
}
