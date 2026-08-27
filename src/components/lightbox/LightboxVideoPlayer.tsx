import { useContainer } from "@videojs/react";
import { I18nProvider } from "@videojs/react/i18n";
import { MinimalVideoSkin, VideoPlayer } from "@videojs/react/video";
import type { CSSProperties, MutableRefObject } from "react";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import type { MpvMediaAdapter } from "./MpvMediaAdapter";
import { MpvMediaComponent } from "./MpvMediaComponent";
import { NATIVE_VIDEO_CONTROL_STRIP_HEIGHT } from "./nativeVideoLayout";

export interface LightboxVideoPlayerHandle {
  toggleFullscreen: () => Promise<void>;
}

interface LightboxVideoPlayerProps {
  assetId: number;
  generation: number;
  title: string;
  aspectRatio: string;
  style?: CSSProperties;
  playerRef: MutableRefObject<LightboxVideoPlayerHandle | null>;
  onLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange: (fullscreen: boolean) => void;
  onError: () => void;
}

interface NativeFullscreenButtonProps {
  adapter: MpvMediaAdapter | null;
  fullscreen: boolean;
  onToggle: () => Promise<void>;
}

function NativeFullscreenButton({
  adapter,
  fullscreen,
  onToggle
}: NativeFullscreenButtonProps) {
  const { t } = useTranslation();
  const container = useContainer();
  const [controlGroup, setControlGroup] = useState<HTMLElement | null>(null);

  useEffect(() => {
    setControlGroup(
      container?.querySelector<HTMLElement>(
        ".media-controls .media-button-group:last-child"
      ) ?? null
    );
  }, [container]);

  if (!controlGroup) return null;

  const label = fullscreen ? t("lightbox.exitFullscreen") : t("lightbox.fullscreen");
  return createPortal(
    <button
      type="button"
      className="media-button media-button--subtle media-button--icon media-button--native-fullscreen"
      data-fullscreen={fullscreen ? "" : undefined}
      aria-label={label}
      title={label}
      aria-pressed={fullscreen}
      disabled={!adapter}
      onClick={() => {
        void onToggle().catch(() => {});
      }}
    >
      <svg className="media-icon" viewBox="0 0 24 24" aria-hidden="true">
        {fullscreen ? (
          <path
            d="M9 4v5H4m16 0h-5V4m0 16v-5h5M4 15h5v5"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          />
        ) : (
          <path
            d="M4 9V4h5m6 0h5v5m0 6v5h-5M9 20H4v-5"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          />
        )}
      </svg>
    </button>,
    controlGroup
  );
}

export function LightboxVideoPlayer({
  assetId,
  generation,
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
  const [adapter, setAdapter] = useState<MpvMediaAdapter | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const handleFullscreenChange = useCallback(
    (nextFullscreen: boolean) => {
      setFullscreen(nextFullscreen);
      onFullscreenChange(nextFullscreen);
    },
    [onFullscreenChange]
  );
  const toggleFullscreen = useCallback(async () => {
    if (!adapter) return;
    if (fullscreen) {
      await adapter.exitFullscreen();
    } else {
      await adapter.requestFullscreen();
    }
  }, [adapter, fullscreen]);

  useEffect(() => {
    if (!adapter) return;
    const handle: LightboxVideoPlayerHandle = {
      toggleFullscreen
    };
    playerRef.current = handle;
    return () => {
      if (playerRef.current === handle) playerRef.current = null;
    };
  }, [adapter, playerRef, toggleFullscreen]);

  const playerStyle = {
    aspectRatio,
    "--native-video-control-strip-height": `${NATIVE_VIDEO_CONTROL_STRIP_HEIGHT}px`
  } as CSSProperties;

  return (
    <div className="h-full w-full max-h-full max-w-full" style={style}>
      <VideoPlayer key={`${assetId}:${generation}`}>
        <I18nProvider locale={locale}>
          <MinimalVideoSkin
            className="lightbox-video-player h-full w-full"
            data-lightbox-video-player
            data-native-video-active
            data-native-fullscreen={fullscreen ? "" : undefined}
            aria-label={title}
            style={playerStyle}
          >
            <MpvMediaComponent
              assetId={assetId}
              generation={generation}
              onAdapter={setAdapter}
              onLoadedMetadata={onLoadedMetadata}
              onFullscreenChange={handleFullscreenChange}
              onError={onError}
            />
            <NativeFullscreenButton
              adapter={adapter}
              fullscreen={fullscreen}
              onToggle={toggleFullscreen}
            />
          </MinimalVideoSkin>
        </I18nProvider>
      </VideoPlayer>
    </div>
  );
}
