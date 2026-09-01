import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { SelectedAsset } from "../../types";
import { LightboxMediaStage } from "./LightboxMediaStage";
import { LightboxToolbar, SIDEBAR_CLOSE_BUTTON_ID } from "./LightboxToolbar";
import { useLightboxModalHandlers } from "./hooks/useLightboxModalHandlers";
import { useLightboxMediaGroupClipboard } from "./hooks/useLightboxMediaGroupClipboard";
import { useLightboxImageControls } from "./useLightboxImageControls";
import { useLightboxTagging } from "./useLightboxTagging";
import { useTranslation } from "react-i18next";
import { useUiLayer } from "../UI/UiLayerProvider";
import { UiAlert } from "../UI/UiAlert";
import { UiIconButton } from "../UI/UiIconButton";

const NARROW_LIGHTBOX_QUERY = "(max-width: 767px)";
const SIDEBAR_TRIGGER_BUTTON_ID = "lightbox-sidebar-trigger-button";

function readNarrowLightbox(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(NARROW_LIGHTBOX_QUERY).matches;
}

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
  const sidebarWasOpenRef = useRef(false);
  const [isNarrow, setIsNarrow] = useState(readNarrowLightbox);
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
    onClose
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
    knownTags
  });

  const mediaControls = useLightboxImageControls({
    keyboardShortcutsEnabled: !handlers.deleteConfirmOpen && !(isNarrow && handlers.sidebarOpen),
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
    closeOnEscape: !mediaControls.isFullscreen && !handlers.deleteConfirmOpen,
    onEscape: () => {
      if (isNarrow && handlers.sidebarOpen) {
        handlers.handleCloseSidebar();
      } else {
        onClose();
      }
    },
    getRestoreFocus
  });

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mediaQuery = window.matchMedia(NARROW_LIGHTBOX_QUERY);
    const updateLayout = () => {
      setIsNarrow(mediaQuery.matches);
      if (mediaQuery.matches) handlers.handleCloseSidebar();
    };
    updateLayout();
    mediaQuery.addEventListener("change", updateLayout);
    return () => mediaQuery.removeEventListener("change", updateLayout);
  }, [handlers.handleCloseSidebar]);

  useEffect(() => {
    if (!isNarrow) {
      sidebarWasOpenRef.current = false;
      return;
    }

    const wasOpen = sidebarWasOpenRef.current;
    sidebarWasOpenRef.current = handlers.sidebarOpen;
    const frame = window.requestAnimationFrame(() => {
      if (handlers.sidebarOpen) {
        document.getElementById(SIDEBAR_CLOSE_BUTTON_ID)?.focus();
      } else if (wasOpen) {
        document.getElementById(SIDEBAR_TRIGGER_BUTTON_ID)?.focus();
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [handlers.sidebarOpen, isNarrow]);
  const groupCopyConfirmed = clipboard.groupCopyConfirmed;
  const copyMediaGroupTitle = groupCopyConfirmed
    ? t("lightbox.copyMediaGroup.copied")
    : clipboard.canCopyMediaGroup
      ? t("lightbox.copyMediaGroup.title")
      : t("lightbox.copyMediaGroup.unavailable");
  const isVideo = selected?.kind === "video";
  const videoFullscreen = isVideo && mediaControls.isFullscreen;
  const imageFullscreen = !isVideo && mediaControls.isFullscreen;
  const hideNativeVideoForSidebar = Boolean(isVideo && isNarrow && handlers.sidebarOpen);

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
        if (!isTopLayer) return;
        if (isNarrow && handlers.sidebarOpen) {
          if (!handlers.deleteConfirmOpen && !handlers.deleteSubmitting) {
            handlers.handleCloseSidebar();
          }
          return;
        }
        mediaControls.tryCloseLightbox();
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
            : [
                "h-[min(calc(100vh-40px),1180px)] min-h-[360px] w-[min(calc(100vw-40px),1800px)]",
                "rounded-[var(--radius-surface)] border shadow-[var(--shadow-modal)]",
                "sm:min-h-[460px] sm:w-[min(calc(100vw-40px),1860px)] sm:rounded-[var(--radius-panel)]",
                "lg:h-[min(calc(100vh-40px),1280px)] lg:w-[min(calc(100vw-40px),1920px)]",
                isVideo
                  ? "lightbox-shell--video border-white/10"
                  : "border-[var(--border-soft)] bg-[var(--surface-solid)]"
              ].join(" ")
        ].join(" ")}
        ref={mediaControls.lightboxShellRef}
        role="dialog"
        aria-modal="true"
        aria-label={t("lightbox.previewDialog", { name: selected.file_name })}
        aria-describedby="lightbox-dialog-description"
        data-lightbox-kind={selected.kind}
        tabIndex={-1}
        onClick={handlers.handleShellClick}
      >
        <span id="lightbox-dialog-description" className="sr-only">
          {t("lightbox.previewDialogDescription")}
        </span>

        {videoFullscreen ? (
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
        ) : (
          <div
            className={[
              "relative grid h-full min-h-0",
              isNarrow
                ? "grid-cols-1 grid-rows-[auto_minmax(0,1fr)]"
                : "grid-cols-[minmax(0,1fr)_clamp(18rem,22vw,22rem)]"
            ].join(" ")}
          >
            {isNarrow ? (
              <header className="relative z-[6] flex min-h-11 items-center justify-between gap-2 border-b border-[var(--border-soft)] bg-[var(--surface-solid)] px-2 py-1.5">
                <span className="min-w-0 truncate text-xs font-semibold text-base-content" title={selected.file_name}>
                  {selected.file_name}
                </span>
                {!handlers.sidebarOpen ? (
                  <div className="flex shrink-0 items-center gap-1.5">
                    <UiIconButton
                      id={SIDEBAR_TRIGGER_BUTTON_ID}
                      icon="bulk-actions"
                      iconClassName="h-4 w-4 shrink-0"
                      className="h-8 w-8 min-h-8"
                      aria-label={t("lightbox.openPanel")}
                      title={t("lightbox.openPanel")}
                      onClick={handlers.handleOpenSidebar}
                    />
                    <UiIconButton
                      icon="close"
                      iconClassName="h-4 w-4 shrink-0"
                      className="h-8 w-8 min-h-8"
                      aria-label={t("lightbox.closePreview")}
                      title={t("common.close")}
                      onClick={handlers.handleCloseLightbox}
                    />
                  </div>
                ) : null}
              </header>
            ) : null}

            <div
              className={[
                "relative h-full min-h-0 min-w-0 overflow-hidden",
                hideNativeVideoForSidebar ? "hidden" : ""
              ].join(" ")}
            >
              {favoriteFailed || handlers.mediaGroupFailed ? (
                <div className="absolute left-3 top-3 z-[5] grid max-w-[min(28rem,calc(100%-6rem))] gap-2">
                  {favoriteFailed ? (
                    <UiAlert
                      tone="error"
                      title={t("lightbox.saveFailedTitle")}
                      className="shadow-[var(--shadow-floating)]"
                    >
                      {t("lightbox.favoriteSaveFailed")}
                    </UiAlert>
                  ) : null}
                  {handlers.mediaGroupFailed ? (
                    <UiAlert
                      tone="error"
                      title={t("lightbox.saveFailedTitle")}
                      className="shadow-[var(--shadow-floating)]"
                    >
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

            {isNarrow && handlers.sidebarOpen ? (
              <div
                className="absolute inset-0 z-[7] bg-neutral/52 backdrop-blur-[1px]"
                data-testid="lightbox-sidebar-scrim"
                onClick={(event) => {
                  event.stopPropagation();
                  if (!handlers.deleteConfirmOpen && !handlers.deleteSubmitting) {
                    handlers.handleCloseSidebar();
                  }
                }}
              />
            ) : null}

            <LightboxToolbar
              selected={selected}
              mediaGroupKeyEditor={mediaGroupKeyEditor}
              mediaGroupOrderEditor={mediaGroupOrderEditor}
              groupCopyConfirmed={groupCopyConfirmed}
              canCopyMediaGroup={clipboard.canCopyMediaGroup}
              copyMediaGroupTitle={copyMediaGroupTitle}
              isNarrow={isNarrow}
              sidebarOpen={handlers.sidebarOpen}
              isFullscreen={mediaControls.isFullscreen}
              selectedTags={tagging.selectedTags}
              tagDraft={tagging.tagDraft}
              knownTags={tagging.availableKnownTags}
              tagInputRef={tagging.tagInputRef}
              tagSaving={tagging.tagSaving}
              tagFailed={tagging.tagFailed}
              tagDetailsLoading={tagDetailsLoading}
              tagDetailsFailed={tagDetailsFailed}
              tagEditingDisabled={tagging.tagEditingDisabled}
              onRemoveTag={tagging.removeTag}
              onTagDraftChange={tagging.setTagDraft}
              onAddTag={tagging.addTag}
              onRetryTags={tagging.retryTags}
              onRetryTagDetails={onRetryTagDetails}
              onMediaGroupKeyChange={onMediaGroupKeyEditorChange}
              onMediaGroupOrderChange={onMediaGroupOrderEditorChange}
              onApplyMediaGroup={handlers.handleApplyMediaGroup}
              infoPanelOpen={handlers.infoPanelOpen}
              onToggleInfo={handlers.handleToggleInfoPanel}
              onCloseSidebar={handlers.handleCloseSidebar}
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
              deleteConfirmOpen={handlers.deleteConfirmOpen}
              deleteSubmitting={handlers.deleteSubmitting}
              deleteError={handlers.deleteError}
              onDeleteConfirmClose={handlers.handleCloseDeleteConfirm}
              onDeleteConfirmSubmit={handlers.handleConfirmDeleteMedia}
              t={t}
            />
          </div>
        )}
      </div>
    </div>,
    document.body
  );
}
