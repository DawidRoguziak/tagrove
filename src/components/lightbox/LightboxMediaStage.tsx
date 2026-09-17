import type {
  MutableRefObject,
  MouseEventHandler,
  PointerEventHandler,
  ReactEventHandler
} from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { VideoBounds } from "./mpvVideoTypes";
import { toMediaSrc } from "../../api";
import type { SelectedAsset } from "../../types";
import { UiButton } from "../UI/UiButton";
import {
  LightboxVideoPlayer,
  type LightboxVideoPlayerHandle
} from "./LightboxVideoPlayer";

interface LightboxMediaStageProps {
  playbackActive?: boolean;
  selected: SelectedAsset;
  detailsLoading?: boolean;
  detailsFailed?: boolean;
  onRetryDetails?: () => void;
  mediaViewportRef: MutableRefObject<HTMLDivElement | null>;
  lightboxImageRef: MutableRefObject<HTMLImageElement | null>;
  lightboxVideoPlayerRef: MutableRefObject<LightboxVideoPlayerHandle | null>;
  isZoomed: boolean;
  mediaDisplaySize: { width: number; height: number };
  isDragging: boolean;
  isFullscreen: boolean;
  onImageLoad: ReactEventHandler<HTMLImageElement>;
  onVideoLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onVideoFullscreenChange: (fullscreen: boolean) => void;
  onPointerActivity?: () => void;
  onNativeBounds?: (bounds: VideoBounds) => void;
  onImageClick: MouseEventHandler<HTMLImageElement>;
  onImagePointerDown: PointerEventHandler<HTMLDivElement>;
  onImagePointerMove: PointerEventHandler<HTMLDivElement>;
  onImagePointerEnd: PointerEventHandler<HTMLDivElement>;
}

export function LightboxMediaStage({
  playbackActive = true,
  selected,
  detailsFailed = false,
  onRetryDetails = () => {},
  mediaViewportRef,
  lightboxImageRef,
  lightboxVideoPlayerRef,
  isZoomed,
  mediaDisplaySize,
  isDragging,
  isFullscreen,
  onImageLoad,
  onVideoLoadedMetadata,
  onVideoFullscreenChange,
  onPointerActivity,
  onNativeBounds,
  onImageClick,
  onImagePointerDown,
  onImagePointerMove,
  onImagePointerEnd
}: LightboxMediaStageProps) {
  const { t } = useTranslation();
  const detailsLoaded = selected.path !== null;
  const mediaKey = `${selected.id}\u0000${selected.kind}\u0000${selected.path ?? ""}`;
  const activationRef = useRef({ mediaKey, generation: 0 });
  if (activationRef.current.mediaKey !== mediaKey) {
    activationRef.current = {
      mediaKey,
      generation: activationRef.current.generation + 1
    };
  }
  const activationGeneration = activationRef.current.generation;
  const [failedActivation, setFailedActivation] = useState<number | null>(null);
  const reportMediaFailure = useCallback(() => {
    if (activationRef.current.generation === activationGeneration) {
      setFailedActivation(activationGeneration);
    }
  }, [activationGeneration]);
  // Without loaded details there is no verified source path: hold the previous
  // media (or a loading placeholder) instead of guessing from summary fields.
  const mediaFailed = failedActivation === activationGeneration;
  useEffect(() => {
    if (selected.kind === "video" && (!detailsLoaded || mediaFailed)) {
      onNativeBounds?.({ x: 0, y: 0, width: 0, height: 0 });
    }
  }, [selected.kind, detailsLoaded, mediaFailed, onNativeBounds]);
  const mediaStyle =
    mediaDisplaySize.width > 0 && mediaDisplaySize.height > 0
      ? {
          width: `${mediaDisplaySize.width}px`,
          height: `${mediaDisplaySize.height}px`
        }
      : undefined;
  const imageStyle = mediaStyle
    ? {
        width: `calc(${mediaStyle.width} * var(--lightbox-image-zoom, 1))`,
        height: `calc(${mediaStyle.height} * var(--lightbox-image-zoom, 1))`,
        maxWidth: "none",
        maxHeight: "none"
      }
    : undefined;

  return (
    <div
      className={`relative h-full min-h-0 min-w-0 w-full overflow-hidden [contain:paint] ${
        selected.kind === "video"
          ? "lightbox-media-stage--video"
          : "bg-[var(--surface-muted)]"
      }`}
      data-lightbox-media-stage={selected.kind}
      ref={(node) => {
        mediaViewportRef.current = node;
      }}
    >
      {mediaFailed ? (
        <div className="flex h-full w-full items-center justify-center p-6">
          <div
            role="alert"
            data-testid="lightbox-media-error"
            className="rounded-[var(--radius-control)] border border-error/52 bg-error/14 px-5 py-4 text-center text-sm font-semibold text-base-content"
          >
            {selected.kind === "video"
              ? t("lightbox.mediaError.video")
              : t("lightbox.mediaError.image")}
            {selected.kind === "video" && (
              <div className="mt-3">
                <UiButton onClick={() => {
                  activationRef.current = { mediaKey, generation: activationRef.current.generation + 1 };
                  setFailedActivation(null);
                }}>{t("gallery.retry")}</UiButton>
              </div>
            )}
          </div>
        </div>
      ) : !detailsLoaded ? (
        detailsFailed ? (
          <div
            role="alert"
            data-testid="lightbox-details-error"
            className="flex h-full w-full items-center justify-center p-6"
          >
            <div className="grid gap-3 rounded-[var(--radius-control)] border border-warning/52 bg-warning/14 px-5 py-4 text-center text-sm font-semibold text-base-content">
              <span>{t("lightbox.assetDetailsLoadFailed")}</span>
              <UiButton onClick={onRetryDetails}>{t("lightbox.retryAssetDetails")}</UiButton>
            </div>
          </div>
        ) : (
          <div
            className="flex h-full w-full items-center justify-center"
            data-testid="lightbox-details-loading"
          >
            <span className="h-[34px] w-[34px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
          </div>
        )
      ) : selected.kind === "video" ? (
        <div className="flex h-full w-full items-center justify-center overflow-hidden">
          {playbackActive && <LightboxVideoPlayer
            style={isFullscreen ? undefined : mediaStyle}
            assetId={selected.id}
            generation={activationGeneration}
            title={selected.path ?? ""}
            aspectRatio={
              mediaDisplaySize.width > 0 && mediaDisplaySize.height > 0
                ? `${mediaDisplaySize.width} / ${mediaDisplaySize.height}`
                : selected.width && selected.height
                  ? `${selected.width} / ${selected.height}`
                  : "16 / 9"
            }
            playerRef={lightboxVideoPlayerRef}
            onLoadedMetadata={onVideoLoadedMetadata}
            onFullscreenChange={onVideoFullscreenChange}
            onPointerActivity={onPointerActivity}
            onNativeBounds={onNativeBounds}
            onError={reportMediaFailure}
          />}
        </div>
      ) : (
        <div
          className={`flex h-full w-full touch-none items-center justify-center overflow-hidden ${
            isZoomed ? (isDragging ? "cursor-grabbing" : "cursor-grab") : ""
          }`}
          onPointerDown={onImagePointerDown}
          onPointerMove={onImagePointerMove}
          onPointerUp={onImagePointerEnd}
          onPointerCancel={onImagePointerEnd}
          onLostPointerCapture={onImagePointerEnd}
        >
          <img
            key={activationGeneration}
            data-testid="lightbox-image"
            ref={(node) => {
              lightboxImageRef.current = node;
            }}
            src={toMediaSrc(selected.path ?? "")}
            alt={selected.file_name}
            style={imageStyle}
            className={`max-h-full max-w-full shrink-0 select-none object-contain ${
              isZoomed ? (isDragging ? "cursor-grabbing" : "cursor-grab") : "cursor-zoom-in"
            }`}
            onLoad={onImageLoad}
            onError={reportMediaFailure}
            onClick={onImageClick}
            draggable={false}
          />
        </div>
      )}
      {detailsLoaded && detailsFailed ? (
        <div
          className={`absolute left-4 z-[3] flex items-center gap-3 rounded-[var(--radius-control)] border border-warning/52 bg-base-100/92 px-3 py-2 text-xs shadow-[var(--shadow-floating)] ${
            selected.kind === "video" ? "bottom-[88px]" : "bottom-4"
          }`}
          role="alert"
        >
           <span>{t("lightbox.assetDetailsLoadFailed")}</span>
           <UiButton className="btn-sm" onClick={onRetryDetails}>{t("lightbox.retryAssetDetails")}</UiButton>
        </div>
      ) : null}
    </div>
  );
}
