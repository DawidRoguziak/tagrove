import { I18nProvider } from "@videojs/react/i18n";
import { MinimalVideoSkin, VideoPlayer } from "@videojs/react/video";
import type { CSSProperties, MutableRefObject } from "react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
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
  const { i18n, t } = useTranslation();
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

  useEffect(() => {
    if (!adapter) return;
    const handle: LightboxVideoPlayerHandle = {
      toggleFullscreen: () =>
        fullscreen ? adapter.exitFullscreen() : adapter.requestFullscreen()
    };
    playerRef.current = handle;
    return () => {
      if (playerRef.current === handle) playerRef.current = null;
    };
  }, [adapter, fullscreen, playerRef]);

  return (
    <div className="h-full w-full max-h-full max-w-full" style={style}>
      <VideoPlayer key={`${assetId}:${generation}`}>
        <I18nProvider locale={locale}>
          <MinimalVideoSkin
            className="lightbox-video-player h-full w-full"
            data-lightbox-video-player
            data-native-video-active
            aria-label={title}
            style={{ aspectRatio }}
          >
            <MpvMediaComponent
              assetId={assetId}
              generation={generation}
              onAdapter={setAdapter}
              onLoadedMetadata={onLoadedMetadata}
              onFullscreenChange={handleFullscreenChange}
              onError={onError}
            />
            <button
              type="button"
              className="media-button media-button--subtle media-button--icon media-button--native-fullscreen"
              aria-label={t("lightbox.fullscreen")}
              onClick={() => {
                if (adapter) {
                  void (fullscreen ? adapter.exitFullscreen() : adapter.requestFullscreen());
                }
              }}
            >
              <svg className="media-icon" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5" fill="none" stroke="currentColor" strokeWidth="2" />
              </svg>
            </button>
          </MinimalVideoSkin>
        </I18nProvider>
      </VideoPlayer>
    </div>
  );
}
