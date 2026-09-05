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
  volume: string;
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
  | { type: "setFullscreen"; fullscreen: boolean }
  | { type: "toggleFullscreen" };

export interface PlaybackSnapshot {
  session_id: number;
  duration: number;
  current_time: number;
  paused: boolean;
  seeking: boolean;
  buffering: boolean;
  volume: number;
  muted: boolean;
  rate: number;
  fullscreen: boolean;
}

export type MpvVideoEvent = { session_id: number } & (
  | { type: "pointerActivity" }
  | { type: "loading" }
  | { type: "metadata"; duration: number; width: number; height: number }
  | { type: "snapshot"; state: PlaybackSnapshot }
  | { type: "fullscreen"; fullscreen: boolean }
  | { type: "error"; message: string }
  | { type: "controlError"; message: string }
);
