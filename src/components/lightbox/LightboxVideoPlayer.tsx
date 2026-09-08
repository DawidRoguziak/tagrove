import type { CSSProperties, MutableRefObject } from "react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { NativeVideoControlLabels, VideoBounds } from "./mpvVideoTypes";
import { useNativeVideoSession } from "./useNativeVideoSession";
import { UiButton } from "../UI/UiButton";

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
  const { t } = useTranslation();
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const controlLabels = useMemo<NativeVideoControlLabels>(
    () => ({
      play: t("lightbox.videoControls.play"),
      pause: t("lightbox.videoControls.pause"),
      mute: t("lightbox.videoControls.mute"),
      unmute: t("lightbox.videoControls.unmute"),
      volume: t("lightbox.videoControls.volume"),
      seek: t("lightbox.videoControls.seek"),
      playbackRate: t("lightbox.videoControls.playbackRate"),
      fullscreen: t("lightbox.fullscreen"),
      exitFullscreen: t("lightbox.exitFullscreen")
    }),
    [t]
  );
  const { sessionId, fullscreen, snapshot, controlError, dismissControlError, toggleFullscreen, sendPlaybackControl } =
    useNativeVideoSession({
      assetId,
      generation,
      container,
      controlLabels,
      onLoadedMetadata,
      onFullscreenChange,
      onError,
      onPointerActivity,
      onNativeBounds
    });

  useEffect(() => {
    if (sessionId === null) return;
    const handle: LightboxVideoPlayerHandle = { toggleFullscreen };
    playerRef.current = handle;
    return () => {
      if (playerRef.current === handle) playerRef.current = null;
    };
  }, [playerRef, sessionId, toggleFullscreen]);

  return (
    <div className="flex h-full w-full max-h-full max-w-full flex-col" style={style}>
      {controlError && (
        <div role="alert" className="flex shrink-0 items-center gap-2 bg-base-100 p-2 text-sm">
          <span className="min-w-0 flex-1">{t("lightbox.videoControls.controlError")}</span>
          <UiButton onClick={dismissControlError}>{t("common.close")}</UiButton>
        </div>
      )}
      <div
        ref={setContainer}
        className="lightbox-video-player min-h-0 w-full flex-1"
        data-lightbox-video-player
        data-native-video-active={sessionId !== null ? "" : undefined}
        data-native-session={sessionId ?? undefined}
        data-native-fullscreen={fullscreen ? "" : undefined}
        data-native-paused={snapshot?.paused}
        data-native-seeking={snapshot?.seeking}
        data-native-buffering={snapshot?.buffering}
        data-native-time={snapshot?.current_time}
        data-native-muted={snapshot?.muted}
        data-native-volume={snapshot?.volume}
        data-native-rate={snapshot?.rate}
        tabIndex={0}
        onClick={(event) => {
          if (event.button !== 0) return;
          event.currentTarget.focus({ preventScroll: true });
          void sendPlaybackControl({ type: "togglePause" });
        }}
        onKeyDown={(event) => {
          if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
          if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
            event.preventDefault();
            event.stopPropagation();
            void sendPlaybackControl({ type: "seekRelative", seconds: event.key === "ArrowLeft" ? -5 : 5 });
          } else if (event.key === " " || event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            if (!event.repeat) void sendPlaybackControl({ type: "togglePause" });
          }
        }}
        aria-label={title}
        style={{ aspectRatio }}
      />
    </div>
  );
}
