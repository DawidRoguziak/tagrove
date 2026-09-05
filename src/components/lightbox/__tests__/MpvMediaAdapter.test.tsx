import { I18nProvider } from "@videojs/react/i18n";
import { MinimalVideoSkin, usePlayer, VideoPlayer } from "@videojs/react/video";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode, useEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MpvMediaAdapter } from "../MpvMediaAdapter";
import { MpvMediaComponent } from "../MpvMediaComponent";
import {
  LightboxVideoPlayer,
  type LightboxVideoPlayerHandle
} from "../LightboxVideoPlayer";
import type { MpvVideoEvent } from "../mpvVideoTypes";

const apiMocks = vi.hoisted(() => ({
  openVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  closeVideo: vi.fn()
}));

vi.mock("../../../api", () => apiMocks);

interface Snapshot {
  paused: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  waiting: boolean;
}

function readStateValue(state: unknown, key: keyof Snapshot): unknown {
  if (typeof state !== "object" || state === null) return undefined;
  if (key === "paused" && "paused" in state) return state.paused;
  if (key === "currentTime" && "currentTime" in state) return state.currentTime;
  if (key === "duration" && "duration" in state) return state.duration;
  if (key === "volume" && "volume" in state) return state.volume;
  if (key === "waiting" && "waiting" in state) return state.waiting;
  return undefined;
}

function readNumber(state: unknown, key: "currentTime" | "duration" | "volume", fallback: number) {
  const value = readStateValue(state, key);
  return typeof value === "number" ? value : fallback;
}

function StateProbe({ onState }: { onState: (state: Snapshot) => void }) {
  const state = usePlayer((value) => ({
    paused: readStateValue(value, "paused") !== false,
    currentTime: readNumber(value, "currentTime", 0),
    duration: readNumber(value, "duration", 0),
    volume: readNumber(value, "volume", 1),
    waiting: readStateValue(value, "waiting") === true
  }));
  useEffect(() => {
    onState(state);
  }, [onState, state]);
  return null;
}

function Prototype({ adapter, onState }: { adapter: MpvMediaAdapter; onState: (state: Snapshot) => void }) {
  return (
    <VideoPlayer>
      <I18nProvider locale="en">
        <MinimalVideoSkin data-lightbox-video-player aria-label="Prototype video">
          <MpvMediaComponent adapter={adapter} />
          <StateProbe onState={onState} />
        </MinimalVideoSkin>
      </I18nProvider>
    </VideoPlayer>
  );
}

function NativePrototype({
  marker,
  onMetadata,
  onError
}: {
  marker: number;
  onMetadata: (marker: number) => void;
  onError?: (message: string) => void;
}) {
  return (
    <VideoPlayer>
      <I18nProvider locale="en">
        <MinimalVideoSkin data-lightbox-video-player aria-label="Native video">
          <MpvMediaComponent
            assetId={123}
            generation={4}
            onLoadedMetadata={() => onMetadata(marker)}
            onFullscreenChange={() => onMetadata(marker)}
            onError={onError}
          />
        </MinimalVideoSkin>
      </I18nProvider>
    </VideoPlayer>
  );
}

beforeEach(() => {
  let sessionId = 0;
  apiMocks.openVideo.mockReset().mockImplementation(async () => {
    sessionId += 1;
    return sessionId;
  });
  apiMocks.setVideoBounds.mockReset().mockResolvedValue(undefined);
  apiMocks.controlVideo.mockReset().mockResolvedValue(undefined);
  apiMocks.closeVideo.mockReset().mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("MpvMediaAdapter Video.js prototype", () => {
  it("forwards only current-session pointer activity and acknowledged bounds", async () => {
    const onActivity = vi.fn();
    const onBounds = vi.fn();
    let acknowledge: () => void = () => {};
    apiMocks.setVideoBounds.mockImplementation(() => new Promise<void>((resolve) => { acknowledge = resolve; }));
    render(
      <VideoPlayer><I18nProvider locale="en"><MinimalVideoSkin>
        <MpvMediaComponent assetId={123} onPointerActivity={onActivity} onNativeBounds={onBounds} />
      </MinimalVideoSkin></I18nProvider></VideoPlayer>
    );
    await waitFor(() => expect(apiMocks.setVideoBounds).toHaveBeenCalled());
    expect(onBounds).not.toHaveBeenCalled();
    const onEvent = apiMocks.openVideo.mock.calls[0][4];
    act(() => {
      onEvent({ session_id: 999, type: "pointerActivity" });
      onEvent({ session_id: 1, type: "pointerActivity" });
    });
    expect(onActivity).toHaveBeenCalledTimes(1);
    await act(async () => acknowledge());
    expect(onBounds).toHaveBeenCalledWith({ x: 0, y: 0, width: 0, height: 0 });
  });

  it("drives MinimalVideoSkin playback, timeline, volume, and waiting state", async () => {
    const states: Snapshot[] = [];
    const adapter = new MpvMediaAdapter("asset:123");
    render(<Prototype adapter={adapter} onState={(state) => states.push(state)} />);

    act(() => adapter.updateMetadata({ duration: 120, width: 1920, height: 1080 }));
    act(() => adapter.updatePlaying());
    act(() => adapter.updateTime(30));
    act(() => adapter.updateVolume(0.35, false));
    act(() => adapter.updateWaiting());

    await waitFor(() => {
      expect(states.at(-1)).toMatchObject({
        paused: false,
        currentTime: 30,
        duration: 120,
        volume: 0.35,
        waiting: true
      });
    });
    expect(document.querySelector("video")).toBeNull();
    expect(document.querySelector(".media-slider")).not.toBeNull();

    act(() => adapter.updatePlaying());
    act(() => adapter.updatePaused());
    await waitFor(() => expect(states.at(-1)?.paused).toBe(true));
  });

  it("maps skin controls back to native commands", async () => {
    const commands = {
      play: vi.fn(),
      pause: vi.fn(),
      seek: vi.fn(),
      setVolume: vi.fn(),
      setMuted: vi.fn(),
      setRate: vi.fn(),
      setFullscreen: vi.fn()
    };
    const adapter = new MpvMediaAdapter("asset:123", commands);
    render(<Prototype adapter={adapter} onState={() => {}} />);
    act(() => adapter.updateMetadata({ duration: 100, width: 1280, height: 720 }));

    fireEvent.click(screen.getByRole("button", { name: /play/i }));
    await waitFor(() => expect(commands.play).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByRole("button", { name: /pause/i }));
    expect(commands.pause).toHaveBeenCalledOnce();

    act(() => {
      adapter.currentTime = 42;
      adapter.volume = 0.4;
      adapter.muted = true;
      adapter.playbackRate = 1.5;
    });
    expect(commands.seek).toHaveBeenCalledWith(42);
    expect(commands.setVolume).toHaveBeenCalledWith(0.4);
    expect(commands.setMuted).toHaveBeenCalledWith(true);
    expect(commands.setRate).toHaveBeenCalledWith(1.5);
  });

  it("exposes fullscreen control while native GTK owns the visible controls", async () => {
    const onFullscreenChange = vi.fn();
    const playerRef: { current: LightboxVideoPlayerHandle | null } = { current: null };
    render(
      <LightboxVideoPlayer
        assetId={123}
        generation={4}
        title="Native video"
        aspectRatio="16 / 9"
        playerRef={playerRef}
        onLoadedMetadata={() => {}}
        onFullscreenChange={onFullscreenChange}
        onError={() => {}}
      />
    );

    await waitFor(() => expect(apiMocks.openVideo).toHaveBeenCalled());
    const activeSession = apiMocks.setVideoBounds.mock.calls.at(-1)?.[0] as number;
    const eventHandler = apiMocks.openVideo.mock.calls.at(-1)?.[4] as
      | ((event: MpvVideoEvent) => void)
      | undefined;
    await waitFor(() => expect(playerRef.current).not.toBeNull());
    expect(document.querySelector("[data-native-video-active]")).not.toBeNull();
    await playerRef.current?.toggleFullscreen();
    await waitFor(() =>
      expect(apiMocks.controlVideo).toHaveBeenCalledWith(activeSession, {
        type: "setFullscreen",
        fullscreen: true
      })
    );

    act(() => {
      eventHandler?.({ session_id: activeSession, type: "fullscreen", fullscreen: true });
    });
    expect(onFullscreenChange).toHaveBeenLastCalledWith(true);
    await playerRef.current?.toggleFullscreen();
    await waitFor(() =>
      expect(apiMocks.controlVideo).toHaveBeenCalledWith(activeSession, {
        type: "setFullscreen",
        fullscreen: false
      })
    );
  });

  it("keeps the current native session across callback changes and metadata rerenders", async () => {
    const onMetadata = vi.fn();
    const { rerender, unmount } = render(
      <StrictMode>
        <NativePrototype marker={1} onMetadata={onMetadata} />
      </StrictMode>
    );

    await waitFor(() => expect(apiMocks.openVideo).toHaveBeenCalled());
    await waitFor(() => expect(apiMocks.setVideoBounds).toHaveBeenCalled());
    const initialOpenCount = apiMocks.openVideo.mock.calls.length;
    const activeSession = apiMocks.setVideoBounds.mock.calls.at(-1)?.[0] as number;
    const latestEventHandler = apiMocks.openVideo.mock.calls.at(-1)?.[4] as
      | ((event: MpvVideoEvent) => void)
      | undefined;
    expect(latestEventHandler).toBeDefined();

    act(() => {
      latestEventHandler?.({
        session_id: activeSession,
        type: "metadata",
        duration: 12,
        width: 640,
        height: 360
      });
    });
    expect(onMetadata).toHaveBeenLastCalledWith(1);

    rerender(
      <StrictMode>
        <NativePrototype marker={2} onMetadata={onMetadata} />
      </StrictMode>
    );
    act(() => {
      latestEventHandler?.({ session_id: activeSession, type: "fullscreen", fullscreen: true });
    });
    expect(onMetadata).toHaveBeenLastCalledWith(2);
    expect(apiMocks.openVideo).toHaveBeenCalledTimes(initialOpenCount);

    unmount();
    await waitFor(() => expect(apiMocks.closeVideo).toHaveBeenCalledWith(activeSession));
  });

  it("closes the session and reports the native reason when initial bounds publication fails", async () => {
    const onError = vi.fn();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    apiMocks.setVideoBounds.mockRejectedValueOnce(new Error("surface unavailable"));

    render(<NativePrototype marker={1} onMetadata={() => {}} onError={onError} />);

    await waitFor(() => expect(onError).toHaveBeenCalledWith("surface unavailable"));
    expect(apiMocks.closeVideo).toHaveBeenCalledWith(1);
    expect(consoleError).toHaveBeenCalledWith(expect.stringContaining("surface unavailable"));
  });
});
