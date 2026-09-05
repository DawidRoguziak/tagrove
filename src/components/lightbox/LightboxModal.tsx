import { useCallback, useEffect, useRef, useState } from "react";
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

import { useLightboxSidebar } from "./hooks/useLightboxSidebar";
import { useLightboxActivity } from "./hooks/useLightboxActivity";
import type { VideoBounds } from "./mpvVideoTypes";

const SIDEBAR_TRIGGER_BUTTON_ID = "lightbox-sidebar-trigger-button";

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
  const restoreTriggerFocusRef = useRef(true);
  const sidebarWasOpenRef = useRef<boolean | null>(null);
  const [nativePanelReady, setNativePanelReady] = useState(false);
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

  const sidebar = useLightboxSidebar(selected !== null, handlers.deleteConfirmOpen || handlers.deleteSubmitting);
  const { isNarrow, sidebarOpen, sidebarOccupied, closeSidebar: requestCloseSidebar } = sidebar;
  const closeSidebar = useCallback((restoreFocus = true) => {
    restoreTriggerFocusRef.current = restoreFocus;
    requestCloseSidebar();
  }, [requestCloseSidebar]);
  const activity = useLightboxActivity(selected !== null);
  const openSidebar = () => {
    if (!sidebarOccupied) setNativePanelReady(false);
    sidebar.openSidebar();
  };

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
    keyboardShortcutsEnabled: !handlers.deleteConfirmOpen && !(isNarrow && sidebarOpen),
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
      if (isNarrow && sidebarOpen) {
        closeSidebar();
      } else {
        onClose();
      }
    },
    getRestoreFocus
  });

  const sidebarVisible = sidebarOpen && (selected?.kind !== "video" || nativePanelReady);

  useEffect(() => {
    const wasOpen = sidebarWasOpenRef.current;
    sidebarWasOpenRef.current = sidebarVisible;
    if (wasOpen === null || wasOpen === sidebarVisible) return;
    const frame = window.requestAnimationFrame(() => {
      if (sidebarVisible) {
        document.getElementById(SIDEBAR_CLOSE_BUTTON_ID)?.focus({ preventScroll: true });
      } else if (wasOpen && !sidebarOpen) {
        const target = restoreTriggerFocusRef.current
          ? document.getElementById(SIDEBAR_TRIGGER_BUTTON_ID)
          : mediaControls.lightboxShellRef.current;
        target?.focus({ preventScroll: true });
        activity.reveal();
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [sidebarVisible, sidebarOpen, activity.reveal, mediaControls.lightboxShellRef]);

  const handleNativeBounds = useCallback((bounds: VideoBounds) => {
    const viewport = mediaControls.mediaViewportRef.current?.getBoundingClientRect();
    if (viewport && (bounds.width === 0 || bounds.x + bounds.width <= viewport.right + 1)) {
      setNativePanelReady(true);
    }
  }, [mediaControls.mediaViewportRef]);
  const groupCopyConfirmed = clipboard.groupCopyConfirmed;
  const copyMediaGroupTitle = groupCopyConfirmed
    ? t("lightbox.copyMediaGroup.copied")
    : clipboard.canCopyMediaGroup
      ? t("lightbox.copyMediaGroup.title")
      : t("lightbox.copyMediaGroup.unavailable");
  const isVideo = selected?.kind === "video";
  const videoFullscreen = isVideo && mediaControls.isFullscreen;
  const imageFullscreen = !isVideo && mediaControls.isFullscreen;
  const hideNativeVideoForSidebar = Boolean(isVideo && isNarrow && sidebarOccupied);

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
        if (isNarrow && sidebarOpen) {
          if (!handlers.deleteConfirmOpen && !handlers.deleteSubmitting) {
            closeSidebar();
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
          "relative min-w-0 overflow-hidden",
          videoFullscreen || imageFullscreen
            ? `h-full min-h-0 w-full rounded-none border-0 shadow-none ${
                videoFullscreen ? "bg-neutral" : "bg-base-100"
              }`
            : [
                "h-[min(calc(100dvh-40px),1180px)] min-h-0 w-[min(calc(100vw-40px),1800px)]",
                "rounded-[var(--radius-surface)] border shadow-[var(--shadow-modal)]",
                "sm:w-[min(calc(100vw-40px),1860px)] sm:rounded-[var(--radius-panel)]",
                "lg:h-[min(calc(100dvh-40px),1280px)] lg:w-[min(calc(100vw-40px),1920px)]",
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
            onPointerActivity={activity.reveal}
            onNativeBounds={handleNativeBounds}
            onImageClick={mediaControls.handleImageClick}
            onImagePointerDown={mediaControls.handleImagePointerDown}
            onImagePointerMove={mediaControls.handleImagePointerMove}
            onImagePointerEnd={mediaControls.handleImagePointerEnd}
          />
        ) : (
          <div
            className={[
              "lightbox-layout relative grid h-full min-h-0 min-w-0 grid-rows-[minmax(0,1fr)]",
              !isNarrow && sidebarOccupied
                ? "grid-cols-[minmax(0,1fr)_clamp(18rem,22vw,22rem)]"
                : isVideo ? "grid-cols-[minmax(0,1fr)_54px]" : "grid-cols-[minmax(0,1fr)]"
            ].join(" ")}
          >
            {!sidebarOpen ? (
              <div className="pointer-events-none absolute right-[10px] top-[10px] z-[9]">
                <UiIconButton
                  id={SIDEBAR_TRIGGER_BUTTON_ID}
                  icon="arrow-left"
                  iconClassName="h-4 w-4 shrink-0"
                  className={`lightbox-panel-trigger pointer-events-auto h-[44px]! w-[44px]! min-h-[44px]! ${activity.visible ? "" : "lightbox-panel-trigger--idle"}`}
                  aria-label={t("lightbox.openPanel")}
                  title={t("lightbox.openPanel")}
                  aria-expanded={false}
                  aria-controls="lightbox-sidebar"
                  onClick={openSidebar}
                />
              </div>
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
                onPointerActivity={activity.reveal}
                onNativeBounds={handleNativeBounds}
                onImageClick={mediaControls.handleImageClick}
                onImagePointerDown={mediaControls.handleImagePointerDown}
                onImagePointerMove={mediaControls.handleImagePointerMove}
                onImagePointerEnd={mediaControls.handleImagePointerEnd}
              />
            </div>

            {isNarrow && sidebarOpen ? (
              <div
                className="absolute inset-0 z-[7] bg-neutral/52 backdrop-blur-[1px]"
                data-testid="lightbox-sidebar-scrim"
                onClick={(event) => {
                  event.stopPropagation();
                  if (!handlers.deleteConfirmOpen && !handlers.deleteSubmitting) {
                    closeSidebar();
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
              sidebarOpen={sidebarOpen}
              sidebarVisible={sidebarVisible}
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
              onCloseSidebar={closeSidebar}
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
