import type {
  MutableRefObject,
  MouseEventHandler,
  PointerEventHandler,
  ReactEventHandler
} from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { MediaPlayerInstance } from "@vidstack/react";
import { toMediaSrc } from "../../api";
import type { Asset } from "../../types";
import { LightboxVideoPlayer } from "./LightboxVideoPlayer";

interface LightboxMediaStageProps {
  selected: Asset;
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
  const [failedMediaKey, setFailedMediaKey] = useState<string | null>(null);
  const mediaKey = `${selected.id}\u0000${selected.kind}\u0000${selected.path}`;
  const mediaFailed = failedMediaKey === mediaKey;
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
      ) : selected.kind === "video" ? (
        <div className="flex h-full w-full items-center justify-center overflow-hidden">
          <LightboxVideoPlayer
            style={isFullscreen ? undefined : mediaStyle}
            src={toMediaSrc(selected.path)}
            title={selected.path}
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
            onError={() => setFailedMediaKey(mediaKey)}
          />
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
            data-testid="lightbox-image"
            ref={(node) => {
              lightboxImageRef.current = node;
            }}
            src={toMediaSrc(selected.path)}
            alt={selected.path}
            style={mediaStyle}
            className={`origin-center shrink-0 select-none object-contain [backface-visibility:hidden] [will-change:transform] ${
              isZoomed ? (isDragging ? "cursor-grabbing" : "cursor-grab") : "cursor-zoom-in"
            }`}
            onLoad={onImageLoad}
            onError={() => setFailedMediaKey(mediaKey)}
            onClick={onImageClick}
            draggable={false}
          />
        </div>
      )}
    </div>
  );
}
