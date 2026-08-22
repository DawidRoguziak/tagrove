import { I18nProvider } from "@videojs/react/i18n";
import { MinimalVideoSkin, Video, VideoPlayer, usePlayer } from "@videojs/react/video";
import type { CSSProperties, MutableRefObject, SyntheticEvent } from "react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";

export interface LightboxVideoPlayerHandle {
  toggleFullscreen: () => Promise<void>;
}

interface LightboxVideoPlayerProps {
  src: string;
  title: string;
  aspectRatio: string;
  style?: CSSProperties;
  playerRef: MutableRefObject<LightboxVideoPlayerHandle | null>;
  onLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange: (fullscreen: boolean) => void;
  onError: () => void;
}

function describeMediaError(error: MediaError | null) {
  const code = error?.code ?? 0;
  const category =
    code === 2
      ? "transport"
      : code === 3
        ? "decode-or-codec"
        : code === 4
          ? "unsupported-source-or-codec"
          : code === 1
            ? "aborted"
            : "unknown";
  return { code, category, message: error?.message ?? "" };
}

function LightboxVideoPlayerAdapter({
  playerRef,
  onFullscreenChange
}: Pick<LightboxVideoPlayerProps, "playerRef" | "onFullscreenChange">) {
  const player = usePlayer();
  const fullscreen = usePlayer(
    (state) =>
      typeof state === "object" &&
      state !== null &&
      "fullscreen" in state &&
      state.fullscreen === true
  );

  useEffect(() => {
    const handle: LightboxVideoPlayerHandle = {
      toggleFullscreen: () => player.toggleFullscreen()
    };
    playerRef.current = handle;

    return () => {
      if (playerRef.current === handle) {
        playerRef.current = null;
      }
    };
  }, [player, playerRef]);

  useEffect(() => {
    onFullscreenChange(fullscreen);
  }, [fullscreen, onFullscreenChange]);

  return null;
}

export function LightboxVideoPlayer({
  src,
  title,
  aspectRatio,
  style,
  playerRef,
  onLoadedMetadata,
  onFullscreenChange,
  onError
}: LightboxVideoPlayerProps) {
  const { i18n } = useTranslation();
  const locale = i18n.resolvedLanguage || i18n.language;

  const reportError = (event: SyntheticEvent<HTMLVideoElement>) => {
    const description = describeMediaError(event.currentTarget.error);
    if (description.code === 1) {
      return;
    }
    console.error("[lightbox] Video playback failed", {
      ...description,
      src,
      path: title,
      event
    });
    onError();
  };

  return (
    <div className="max-h-full max-w-full" style={style}>
      <VideoPlayer key={src}>
        <I18nProvider locale={locale}>
          <MinimalVideoSkin
            className="lightbox-video-player h-full w-full"
            data-lightbox-video-player
            aria-label={title}
            style={{ aspectRatio }}
          >
            <Video
              ref={(video) => {
                if (video) {
                  video.muted = false;
                  video.volume = 1;
                }
              }}
              src={src}
              autoPlay
              muted={false}
              loop
              playsInline
              preload="metadata"
              disablePictureInPicture
              onCanPlay={(event) => {
                const video = event.currentTarget;
                if (!video.paused) {
                  return;
                }

                video.muted = false;
                video.volume = 1;
                void video.play().catch(() => {});
              }}
              onLoadedMetadata={(event) => {
                const video = event.currentTarget;
                onLoadedMetadata({
                  width: video.videoWidth,
                  height: video.videoHeight
                });
              }}
              onError={reportError}
            />
            <LightboxVideoPlayerAdapter
              playerRef={playerRef}
              onFullscreenChange={onFullscreenChange}
            />
          </MinimalVideoSkin>
        </I18nProvider>
      </VideoPlayer>
    </div>
  );
}
