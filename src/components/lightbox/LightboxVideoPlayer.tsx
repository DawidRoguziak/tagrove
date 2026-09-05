import { I18nProvider } from "@videojs/react/i18n";
import { MinimalVideoSkin, VideoPlayer } from "@videojs/react/video";
import type { CSSProperties, MutableRefObject } from "react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { NativeVideoControlLabels, VideoBounds } from "./mpvVideoTypes";
import type { MpvMediaAdapter } from "./MpvMediaAdapter";
import { MpvMediaComponent } from "./MpvMediaComponent";

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
  onPointerActivity?: () => void;
  onNativeBounds?: (bounds: VideoBounds) => void;
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
  onError,
  onPointerActivity,
  onNativeBounds
}: LightboxVideoPlayerProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage || i18n.language;
  const [adapter, setAdapter] = useState<MpvMediaAdapter | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const controlLabels = useMemo<NativeVideoControlLabels>(
    () => ({
      play: t("lightbox.videoControls.play"),
      pause: t("lightbox.videoControls.pause"),
      mute: t("lightbox.videoControls.mute"),
      unmute: t("lightbox.videoControls.unmute"),
      seek: t("lightbox.videoControls.seek"),
      playbackRate: t("lightbox.videoControls.playbackRate"),
      fullscreen: t("lightbox.fullscreen"),
      exitFullscreen: t("lightbox.exitFullscreen")
    }),
    [t]
  );
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

  const playerStyle = { aspectRatio } as CSSProperties;

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
              controlLabels={controlLabels}
              onAdapter={setAdapter}
              onLoadedMetadata={onLoadedMetadata}
              onFullscreenChange={handleFullscreenChange}
              onError={onError}
              onPointerActivity={onPointerActivity}
              onNativeBounds={onNativeBounds}
            />
          </MinimalVideoSkin>
        </I18nProvider>
      </VideoPlayer>
    </div>
  );
}
