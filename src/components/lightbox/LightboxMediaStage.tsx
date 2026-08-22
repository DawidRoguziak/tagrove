import type {
  MutableRefObject,
  MouseEventHandler,
  PointerEventHandler,
  ReactEventHandler
} from "react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { MediaPlayerInstance } from "@vidstack/react";
import { toMediaSrc } from "../../api";
import type { SelectedAsset } from "../../types";
import { LightboxVideoPlayer } from "./LightboxVideoPlayer";
import { useLightboxVideoSource } from "./hooks/useLightboxVideoSource";

interface LightboxMediaStageProps {
  selected: SelectedAsset;
  detailsLoading?: boolean;
  detailsFailed?: boolean;
  mediaViewportRef: MutableRefObject<HTMLDivElement | null>;
  lightboxImageRef: MutableRefObject<HTMLImageElement | null>;
  lightboxVideoPlayerRef: MutableRefObject<MediaPlayerInstance | null>;
  isZoomed: boolean;
  mediaDisplaySize: { width: number; height: number };
  isDragging: boolean;
  isFullscreen: boolean;
  onImageLoad: ReactEventHandler<HTMLImageElement>;
  onVideoLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onVideoFullscreenChange: (fullscreen: boolean) => void;
  onImageClick: MouseEventHandler<HTMLImageElement>;
  onImagePointerDown: PointerEventHandler<HTMLDivElement>;
  onImagePointerMove: PointerEventHandler<HTMLDivElement>;
  onImagePointerEnd: PointerEventHandler<HTMLDivElement>;
}

export function LightboxMediaStage({
  selected,
  detailsFailed = false,
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
  const reportMediaFailure = () => {
    if (activationRef.current.generation === activationGeneration) {
      setFailedActivation(activationGeneration);
    }
  };
  const videoSource = useLightboxVideoSource(
    selected.id,
    selected.path,
    selected.kind === "video"
  );
  // Without loaded details there is no verified source path: hold the previous
  // media (or a loading placeholder) instead of guessing from summary fields.
  const mediaFailed = failedActivation === activationGeneration || videoSource.failed;
  const mediaStyle =
    mediaDisplaySize.width > 0 && mediaDisplaySize.height > 0
      ? {
          width: `${mediaDisplaySize.width}px`,
          height: `${mediaDisplaySize.height}px`
        }
      : undefined;

  return (
    <div
      className="relative h-full min-h-0 w-full overflow-hidden bg-[radial-gradient(circle_at_center,oklch(var(--b2)/0.72),oklch(var(--b3)/0.98))] [contain:paint]"
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
          </div>
        </div>
      ) : !detailsLoaded ? (
        detailsFailed ? (
          <div
            role="alert"
            data-testid="lightbox-details-error"
            className="flex h-full w-full items-center justify-center p-6"
          >
            <div className="rounded-[var(--radius-control)] border border-warning/52 bg-warning/14 px-5 py-4 text-center text-sm font-semibold text-base-content">
              {t("lightbox.tagDetailsLoadFailed")}
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
      ) : selected.kind === "video" && videoSource.src ? (
        <div className="flex h-full w-full items-center justify-center overflow-hidden">
          <LightboxVideoPlayer
            style={isFullscreen ? undefined : mediaStyle}
            src={videoSource.src}
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
            onError={reportMediaFailure}
          />
        </div>
      ) : selected.kind !== "video" ? (
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
            data-testid="lightbox-image"
            ref={(node) => {
              lightboxImageRef.current = node;
            }}
            src={toMediaSrc(selected.path ?? "")}
            alt={selected.file_name}
            style={mediaStyle}
            className={`origin-center shrink-0 select-none object-contain [backface-visibility:hidden] [will-change:transform] ${
              isZoomed ? (isDragging ? "cursor-grabbing" : "cursor-grab") : "cursor-zoom-in"
            }`}
            onLoad={onImageLoad}
            onError={reportMediaFailure}
            onClick={onImageClick}
            draggable={false}
          />
        </div>
      ) : null}
    </div>
  );
}
