import { TypedEventTarget } from "@videojs/media";
import type {
  AudioTrackLike,
  AudioTrackListLike,
  ErrorLike,
  EventLike,
  MediaContentData,
  MediaPreloadType,
  MediaReadyStateValue,
  RemotePlaybackLike,
  TextTrackKind,
  TextTrackLike,
  TextTrackListLike,
  TimeRangeLike,
  Video
} from "@videojs/media";
import type { MpvVideoEvent } from "./mpvVideoTypes";

const emptyTimeRanges: TimeRangeLike = {
  length: 0,
  start: () => {
    throw new DOMException("The index is not in the allowed range", "IndexSizeError");
  },
  end: () => {
    throw new DOMException("The index is not in the allowed range", "IndexSizeError");
  }
};

class EmptyTextTrackList extends EventTarget implements TextTrackListLike {
  readonly [index: number]: TextTrackLike;
  readonly length = 0;

  *[Symbol.iterator](): Iterator<TextTrackLike> {}

  getTrackById(_id: string): TextTrackLike | null {
    return null;
  }
}

interface AudioTrackListEvents {
  addtrack: EventLike & { readonly track: AudioTrackLike };
  removetrack: EventLike & { readonly track: AudioTrackLike };
  change: EventLike;
}

class EmptyAudioTrackList
  extends TypedEventTarget<AudioTrackListEvents>()
  implements AudioTrackListLike
{
  readonly [index: number]: AudioTrackLike;
  readonly length = 0;

  *[Symbol.iterator](): Iterator<AudioTrackLike> {}

  getTrackById(_id: string): AudioTrackLike | null {
    return null;
  }
}

const unavailableRemote: RemotePlaybackLike = {
  state: "disconnected",
  addEventListener: () => {},
  removeEventListener: () => {},
  dispatchEvent: () => true,
  prompt: () => Promise.reject(new Error("Remote playback is unavailable")),
  watchAvailability: () => Promise.resolve(0),
  cancelWatchAvailability: () => Promise.resolve()
};

export interface MpvMediaCommands {
  play(): Promise<void> | void;
  pause(): Promise<void> | void;
  seek(time: number): Promise<void> | void;
  setVolume(volume: number): Promise<void> | void;
  setMuted(muted: boolean): Promise<void> | void;
  setRate(rate: number): Promise<void> | void;
  setFullscreen(fullscreen: boolean): Promise<void> | void;
}

const noCommands: MpvMediaCommands = {
  play: () => {},
  pause: () => {},
  seek: () => {},
  setVolume: () => {},
  setMuted: () => {},
  setRate: () => {},
  setFullscreen: () => {}
};

export interface MpvMetadata {
  duration: number;
  width: number;
  height: number;
}

export class MpvMediaAdapter extends EventTarget implements Video {
  readonly audioTracks: AudioTrackListLike = new EmptyAudioTrackList();
  readonly textTracks: TextTrackListLike = new EmptyTextTrackList();
  readonly buffered = emptyTimeRanges;
  readonly played = emptyTimeRanges;
  readonly remote = unavailableRemote;

  controls = false;
  autoplay = true;
  loop = true;
  preload: MediaPreloadType = "metadata";
  crossOrigin: string | null = null;
  defaultMuted = false;
  defaultPlaybackRate = 1;
  disableRemotePlayback = true;
  disablePictureInPicture = true;
  playsInline = true;
  poster = "";
  streamType = "on-demand" as const;
  contentData: MediaContentData = {};

  private source = "";
  private time = 0;
  private mediaDuration = Number.NaN;
  private mediaVolume = 1;
  private mediaMuted = false;
  private rate = 1;
  private mediaPaused = true;
  private mediaEnded = false;
  private mediaSeeking = false;
  private state: MediaReadyStateValue | number = 0;
  private width = 0;
  private height = 0;
  private mediaError: ErrorLike | null = null;
  private fullscreen = false;

  constructor(
    src: string,
    private readonly commands: MpvMediaCommands = noCommands
  ) {
    super();
    this.source = src;
  }

  get src() {
    return this.source;
  }

  set src(value: string) {
    if (value === this.source) return;
    this.source = value;
    this.empty();
    if (value) this.emit("loadstart");
  }

  get currentSrc() {
    return this.source;
  }

  get currentTime() {
    return this.time;
  }

  set currentTime(value: number) {
    if (!Number.isFinite(value)) return;
    const duration = Number.isFinite(this.mediaDuration) ? this.mediaDuration : Number.POSITIVE_INFINITY;
    const next = Math.max(0, Math.min(value, duration));
    this.mediaSeeking = true;
    this.time = next;
    this.emit("seeking");
    void this.commands.seek(next);
  }

  get duration() {
    return this.mediaDuration;
  }

  get volume() {
    return this.mediaVolume;
  }

  set volume(value: number) {
    const next = Math.max(0, Math.min(1, value));
    if (!Number.isFinite(next) || next === this.mediaVolume) return;
    this.mediaVolume = next;
    void this.commands.setVolume(next);
    this.emit("volumechange");
  }

  get muted() {
    return this.mediaMuted;
  }

  set muted(value: boolean) {
    if (value === this.mediaMuted) return;
    this.mediaMuted = value;
    void this.commands.setMuted(value);
    this.emit("volumechange");
  }

  get playbackRate() {
    return this.rate;
  }

  set playbackRate(value: number) {
    if (!Number.isFinite(value) || value <= 0 || value === this.rate) return;
    this.rate = value;
    void this.commands.setRate(value);
    this.emit("ratechange");
  }

  get paused() {
    return this.mediaPaused;
  }

  get ended() {
    return this.mediaEnded;
  }

  get seeking() {
    return this.mediaSeeking;
  }

  get readyState() {
    return this.state;
  }

  get videoWidth() {
    return this.width;
  }

  get videoHeight() {
    return this.height;
  }

  get error() {
    return this.mediaError;
  }

  get seekable(): TimeRangeLike {
    if (!Number.isFinite(this.mediaDuration) || this.mediaDuration <= 0) return emptyTimeRanges;
    const duration = this.mediaDuration;
    return { length: 1, start: (index) => this.rangeValue(index, 0), end: (index) => this.rangeValue(index, duration) };
  }

  get liveEdgeStart() {
    return Number.NaN;
  }

  get targetLiveWindow() {
    return Number.NaN;
  }

  get isFullscreen() {
    return this.fullscreen;
  }

  readonly isPictureInPicture = false;

  async play(): Promise<void> {
    this.mediaPaused = false;
    this.mediaEnded = false;
    this.emit("play");
    await this.commands.play();
  }

  pause(): void {
    if (this.mediaPaused) return;
    this.mediaPaused = true;
    void this.commands.pause();
    this.emit("pause");
  }

  load(): void {
    this.emit("loadstart");
  }

  canPlayType(_type: string) {
    return "probably" as const;
  }

  addTextTrack(_kind: TextTrackKind, _label?: string, _language?: string): TextTrackLike {
    throw new Error("Adding text tracks in the WebView is unsupported");
  }

  async requestFullscreen(): Promise<void> {
    await this.commands.setFullscreen(true);
  }

  async exitFullscreen(): Promise<void> {
    await this.commands.setFullscreen(false);
  }

  requestPictureInPicture(): Promise<never> {
    return Promise.reject(new Error("Picture-in-picture is unavailable"));
  }

  exitPictureInPicture(): Promise<never> {
    return Promise.reject(new Error("Picture-in-picture is unavailable"));
  }

  updateLoading(): void {
    this.state = 0;
    this.mediaPaused = true;
    this.mediaEnded = false;
    this.mediaError = null;
    this.emit("loadstart");
  }

  updateMetadata({ duration, width, height }: MpvMetadata): void {
    this.mediaDuration = duration;
    this.width = width;
    this.height = height;
    this.state = 1;
    this.emit("durationchange");
    this.emit("resize");
    this.emit("loadedmetadata");
    this.state = 4;
    this.emit("canplay");
  }

  updatePlaying(): void {
    this.mediaPaused = false;
    this.mediaEnded = false;
    this.state = 4;
    this.emit("playing");
  }

  updatePaused(): void {
    this.mediaPaused = true;
    this.emit("pause");
  }

  updateWaiting(): void {
    this.state = 2;
    this.emit("waiting");
  }

  updateTime(currentTime: number): void {
    this.time = Math.max(0, currentTime);
    this.emit("timeupdate");
  }

  updateSeeked(currentTime: number): void {
    this.time = Math.max(0, currentTime);
    this.mediaSeeking = false;
    this.emit("timeupdate");
    this.emit("seeked");
  }

  updateVolume(volume: number, muted: boolean): void {
    this.mediaVolume = Math.max(0, Math.min(1, volume));
    this.mediaMuted = muted;
    this.emit("volumechange");
  }

  updateRate(rate: number): void {
    this.rate = rate;
    this.emit("ratechange");
  }

  updateFullscreen(fullscreen: boolean): void {
    this.fullscreen = fullscreen;
    document.dispatchEvent(new Event("fullscreenchange"));
  }

  updateEnded(): void {
    this.mediaEnded = true;
    this.mediaPaused = true;
    this.emit("ended");
  }

  updateError(error: ErrorLike): void {
    this.mediaError = error;
    this.emit("error");
  }

  applyBackendEvent(event: MpvVideoEvent): void {
    switch (event.type) {
      case "loading":
        this.updateLoading();
        break;
      case "metadata":
        this.updateMetadata({ duration: event.duration, width: event.width, height: event.height });
        break;
      case "playing":
        this.updatePlaying();
        break;
      case "paused":
        this.updatePaused();
        break;
      case "waiting":
        this.updateWaiting();
        break;
      case "time":
        this.updateTime(event.current_time);
        break;
      case "volume":
        this.updateVolume(event.volume, event.muted);
        break;
      case "rate":
        this.updateRate(event.rate);
        break;
      case "fullscreen":
        this.updateFullscreen(event.fullscreen);
        break;
      case "ended":
        this.updateEnded();
        break;
      case "error":
        this.updateError({ code: 3, message: event.message });
        break;
      case "pointerActivity":
      case "tracks":
        break;
    }
  }

  empty(): void {
    this.time = 0;
    this.mediaDuration = Number.NaN;
    this.mediaPaused = true;
    this.mediaEnded = false;
    this.mediaSeeking = false;
    this.state = 0;
    this.width = 0;
    this.height = 0;
    this.mediaError = null;
    this.emit("emptied");
  }

  private emit(type: string): void {
    this.dispatchEvent(new Event(type));
  }

  private rangeValue(index: number, value: number): number {
    if (index !== 0) throw new DOMException("The index is not in the allowed range", "IndexSizeError");
    return value;
  }
}
