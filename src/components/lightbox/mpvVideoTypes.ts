export interface VideoBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NativeVideoControlLabels {
  play: string;
  pause: string;
  mute: string;
  unmute: string;
  seek: string;
  playbackRate: string;
  fullscreen: string;
  exitFullscreen: string;
}

export type VideoControl =
  | { type: "play" }
  | { type: "pause" }
  | { type: "seek"; time: number }
  | { type: "setVolume"; volume: number }
  | { type: "setMuted"; muted: boolean }
  | { type: "setRate"; rate: number }
  | { type: "selectAudioTrack"; trackId: string }
  | { type: "selectSubtitleTrack"; trackId: string }
  | { type: "setFullscreen"; fullscreen: boolean };

export type MpvVideoEvent = { session_id: number } & (
  | { type: "pointerActivity" }
  | { type: "loading" }
  | { type: "metadata"; duration: number; width: number; height: number }
  | { type: "playing" }
  | { type: "paused" }
  | { type: "waiting" }
  | { type: "time"; current_time: number }
  | { type: "volume"; volume: number; muted: boolean }
  | { type: "rate"; rate: number }
  | { type: "tracks" }
  | { type: "fullscreen"; fullscreen: boolean }
  | { type: "ended" }
  | { type: "error"; message: string }
);
