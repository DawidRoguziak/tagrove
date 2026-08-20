import {
  MediaPlayer,
  MediaProvider,
  isVideoProvider,
  type MediaPlayerInstance
} from "@vidstack/react";
import {
  DefaultVideoLayout,
  defaultLayoutIcons
} from "@vidstack/react/player/layouts/default";
import type { CSSProperties, MediaHTMLAttributes, MutableRefObject } from "react";

const videoControlsSpacer = (
  <span className="lightbox-video-controls-spacer" aria-hidden="true" />
);

if (typeof window.matchMedia !== "function") {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false
  });
}

interface LightboxVideoPlayerProps {
  src: string;
  title: string;
  aspectRatio: string;
  style?: CSSProperties;
  playerRef: MutableRefObject<MediaPlayerInstance | null>;
  onLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange: (fullscreen: boolean) => void;
  onError: () => void;
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
  return (
    <div className="max-h-full max-w-full" style={style}>
      <MediaPlayer
        key={src}
        ref={(node) => {
          playerRef.current = node;
        }}
        className="lightbox-video-player h-full w-full"
        data-lightbox-video-player
        src={src}
        title=""
        ariaLabel={title}
        aspectRatio={aspectRatio}
        autoPlay
        muted={false}
        volume={1}
        loop
        playsInline
        preload="metadata"
        onCanPlay={() => {
          const player = playerRef.current;
          if (!player || !player.state.paused) {
            return;
          }

          player.muted = false;
          player.volume = 1;
          void player.play().catch(() => {});
        }}
        onLoadedMetadata={() => {
          const provider = playerRef.current?.provider;
          if (!isVideoProvider(provider)) {
            return;
          }

          onLoadedMetadata({
            width: provider.video.videoWidth,
            height: provider.video.videoHeight
          });
        }}
        onFullscreenChange={onFullscreenChange}
        onError={onError}
      >
        <MediaProvider
          mediaProps={{
            loop: true,
            muted: false,
            volume: 1,
            disablePictureInPicture: true,
            onError
          } as MediaHTMLAttributes<HTMLMediaElement>}
        />
        <DefaultVideoLayout
          icons={defaultLayoutIcons}
          noAudioGain
          slots={{
            chapterTitle: videoControlsSpacer,
            googleCastButton: null,
            pipButton: null
          }}
        />
      </MediaPlayer>
    </div>
  );
}
