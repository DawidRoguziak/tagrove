import { createEffect, createMemo, createSignal } from "solid-js";
import type { JSX } from "@solidjs/web";
import { useLanguage } from "../../../i18n/solid";
import { UiButton } from "../../../ui/controls/UiButton";
import { createNativeVideoSession, type SessionOptions } from "./createNativeVideoSession";

export interface LightboxVideoPlayerHandle {
  toggleFullscreen: () => Promise<void>;
}

export function LightboxVideoPlayer(
  props: Omit<SessionOptions, "container" | "controlLabels"> & {
    title: string;
    aspectRatio: string;
    style?: JSX.CSSProperties;
    playerRef: (handle: LightboxVideoPlayerHandle | null) => void;
  }
) {
  const { t } = useLanguage();
  const [container, setContainer] = createSignal<HTMLDivElement | null>(null);
  const labels = createMemo(() => ({
    play: t("lightbox.videoControls.play"),
    pause: t("lightbox.videoControls.pause"),
    mute: t("lightbox.videoControls.mute"),
    unmute: t("lightbox.videoControls.unmute"),
    volume: t("lightbox.videoControls.volume"),
    seek: t("lightbox.videoControls.seek"),
    playbackRate: t("lightbox.videoControls.playbackRate"),
    fullscreen: t("lightbox.fullscreen"),
    exitFullscreen: t("lightbox.exitFullscreen")
  }));
  const session = createNativeVideoSession({
    get assetId() {
      return props.assetId;
    },
    get generation() {
      return props.generation;
    },
    get container() {
      return container();
    },
    get layoutRoot() {
      return props.layoutRoot;
    },
    get controlLabels() {
      return labels();
    },
    onLoadedMetadata: (size) => props.onLoadedMetadata(size),
    onFullscreenChange: (value) => props.onFullscreenChange(value),
    onError: () => props.onError(),
    onEscape: () => props.onEscape?.(),
    onPointerActivity: () => props.onPointerActivity?.(),
    onNativeBounds: (bounds) => props.onNativeBounds?.(bounds)
  });
  createEffect(
    () => session.sessionId,
    (id) => {
      if (id === null) return;
      props.playerRef({ toggleFullscreen: session.toggleFullscreen });
      return () => props.playerRef(null);
    }
  );
  return (
    <div class="flex h-full w-full max-h-full max-w-full flex-col" style={props.style}>
      {session.controlError && (
        <div role="alert" class="flex shrink-0 items-center gap-2 bg-base-100 p-2 text-sm">
          <span class="min-w-0 flex-1">{t("lightbox.videoControls.controlError")}</span>
          <UiButton onClick={session.dismissControlError}>{t("common.close")}</UiButton>
        </div>
      )}
      <div
        ref={setContainer}
        class="lightbox-video-player min-h-0 w-full flex-1"
        data-lightbox-video-player
        data-native-video-active={session.sessionId !== null ? "" : undefined}
        data-native-session={session.sessionId ?? undefined}
        data-native-fullscreen={session.fullscreen ? "" : undefined}
        data-native-paused={session.snapshot ? String(session.snapshot.paused) : undefined}
        data-native-seeking={session.snapshot ? String(session.snapshot.seeking) : undefined}
        data-native-buffering={session.snapshot ? String(session.snapshot.buffering) : undefined}
        data-native-time={session.snapshot?.current_time}
        data-native-muted={session.snapshot ? String(session.snapshot.muted) : undefined}
        data-native-volume={session.snapshot?.volume}
        data-native-rate={session.snapshot?.rate}
        tabindex={0}
        onClick={(event) => {
          if (event.button !== 0) return;
          event.currentTarget.focus({ preventScroll: true });
          void session.sendPlaybackControl({ type: "togglePause" });
        }}
        onKeyDown={(event) => {
          if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
          if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
            event.preventDefault();
            event.stopPropagation();
            void session.sendPlaybackControl({
              type: "seekRelative",
              seconds: event.key === "ArrowLeft" ? -5 : 5
            });
          } else if (event.key === " " || event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            if (!event.repeat) void session.sendPlaybackControl({ type: "togglePause" });
          }
        }}
        aria-label={props.title}
        style={{ "aspect-ratio": props.aspectRatio }}
      />
    </div>
  );
}
