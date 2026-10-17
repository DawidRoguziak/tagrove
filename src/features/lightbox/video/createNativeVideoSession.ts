import { createEffect, createMemo, createSignal, untrack } from "solid-js";
import {
  beginVideoOpen,
  cancelVideoOpen,
  closeVideo,
  controlVideo,
  openVideo,
  setVideoBounds,
  setVideoControlLabels
} from "../../../platform/tauri/video";
import { measureNativeVideoBounds } from "./nativeVideoLayout";
import type {
  MpvVideoEvent,
  NativeVideoControlLabels,
  PlaybackSnapshot,
  VideoBounds,
  VideoControl
} from "../../../platform/tauri/contracts/video";

export interface SessionOptions {
  assetId: number;
  generation: number;
  container: HTMLElement | null;
  controlLabels: NativeVideoControlLabels;
  onLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange: (fullscreen: boolean) => void;
  onError: () => void;
  onEscape?: () => void;
  onPointerActivity?: () => void;
  onNativeBounds?: (bounds: VideoBounds) => void;
}

interface Session {
  id: number;
}

export function createNativeVideoSession(options: SessionOptions) {
  let session: Session | null = null;
  const [sessionId, setSessionId] = createSignal<number | null>(null);
  const [fullscreen, setFullscreen] = createSignal(false);
  const [snapshot, setSnapshot] = createSignal<PlaybackSnapshot | null>(null);
  const [controlError, setControlError] = createSignal<string | null>(null);

  const identity = createMemo<readonly [number, number, HTMLElement | null]>((previous) => {
    const next = [options.assetId, options.generation, options.container] as const;
    return previous && next.every((value, index) => value === previous[index]) ? previous : next;
  });
  createEffect(identity, ([assetId, _generation, container]) => {
    if (!container) return;
    let cancelled = false;
    let requestId: number | null = null;
    let current: Session | null = null;
    let frame: number | null = null;
    let sendingBounds = false;
    let pendingBounds: VideoBounds | null = null;
    const pendingEvents = new Map<MpvVideoEvent["type"], MpvVideoEvent>();
    setSessionId(null);
    setFullscreen(false);
    untrack(() => options.onFullscreenChange(false));
    setSnapshot(null);
    setControlError(null);

    const fail = (error: unknown) => {
      if (cancelled) return;
      console.error(`[native-video] asset ${assetId}: ${errorMessage(error)}`);
      cancelled = true;
      if (current) {
        if (session === current) session = null;
        void closeVideo(current.id).catch(reportCleanupError);
      }
      if (requestId !== null) void cancelVideoOpen(requestId).catch(reportCleanupError);
      setSessionId(null);
      untrack(() => options.onFullscreenChange(false));
      options.onError();
    };
    const publishBounds = async () => {
      if (sendingBounds || !current || cancelled) return;
      sendingBounds = true;
      try {
        while (pendingBounds && !cancelled) {
          const bounds = pendingBounds;
          pendingBounds = null;
          await setVideoBounds(current.id, bounds);
          if (!cancelled) options.onNativeBounds?.(bounds);
        }
      } catch (error) {
        // A failed resize leaves native widgets over unrelated DOM controls.
        fail(error);
      } finally {
        sendingBounds = false;
      }
    };
    const scheduleBounds = () => {
      if (frame !== null || cancelled) return;
      frame = requestAnimationFrame(() => {
        frame = null;
        pendingBounds = measureNativeVideoBounds(container);
        void publishBounds();
      });
    };
    const applyEvent = (event: MpvVideoEvent) => {
      if (cancelled) return;
      if (!current) {
        pendingEvents.set(event.type, event);
        return;
      }
      if (event.session_id !== current.id) return;
      switch (event.type) {
        case "metadata":
          if (event.width > 0 && event.height > 0) {
            options.onLoadedMetadata({ width: event.width, height: event.height });
          }
          break;
        case "snapshot":
          setSnapshot(event.state);
          break;
        case "fullscreen":
          setFullscreen(event.fullscreen);
          options.onFullscreenChange(event.fullscreen);
          scheduleBounds();
          break;
        case "escape":
          options.onEscape?.();
          break;
        case "pointerActivity":
          options.onPointerActivity?.();
          break;
        case "error":
          fail(event.message);
          break;
        case "controlError":
          setControlError(event.message);
          break;
        case "loading":
          break;
      }
    };

    void (async () => {
      try {
        requestId = await beginVideoOpen();
        if (cancelled) {
          await cancelVideoOpen(requestId);
          return;
        }
        const id = await openVideo(
          assetId,
          requestId,
          measureNativeVideoBounds(container),
          options.controlLabels,
          applyEvent
        );
        if (cancelled) {
          await closeVideo(id);
          return;
        }
        current = { id };
        session = current;
        setSessionId(id);
        for (const event of pendingEvents.values()) applyEvent(event);
        pendingEvents.clear();
        pendingBounds = measureNativeVideoBounds(container);
        await publishBounds();
      } catch (error) {
        fail(error);
      }
    })();

    const observer = new ResizeObserver(scheduleBounds);
    observer.observe(container);
    window.addEventListener("resize", scheduleBounds);
    return () => {
      cancelled = true;
      if (frame !== null) cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", scheduleBounds);
      if (session === current) session = null;
      if (requestId !== null) void cancelVideoOpen(requestId).catch(reportCleanupError);
      if (current) void closeVideo(current.id).catch(reportCleanupError);
    };
  });

  createEffect(
    () => ({ id: sessionId(), labels: options.controlLabels }),
    ({ id, labels }) => {
      if (id === null) return;
      let cancelled = false;
      void setVideoControlLabels(id, labels).catch((error) => {
        if (!cancelled && session?.id === id) setControlError(errorMessage(error));
      });
      return () => {
        cancelled = true;
      };
    }
  );

  let controlQueue = Promise.resolve();
  const sendPlaybackControl = (command: VideoControl) => {
    const target = session;
    const operation = controlQueue.then(async () => {
      if (!target || session !== target) return;
      try {
        await controlVideo(target.id, command);
      } catch (error) {
        if (session === target) setControlError(errorMessage(error));
      }
    });
    controlQueue = operation;
    return operation;
  };

  const toggleFullscreen = () => sendPlaybackControl({ type: "toggleFullscreen" });

  return {
    get sessionId() {
      return sessionId();
    },
    get fullscreen() {
      return fullscreen();
    },
    get snapshot() {
      return snapshot();
    },
    get controlError() {
      return controlError();
    },
    dismissControlError: () => setControlError(null),
    sendPlaybackControl,
    toggleFullscreen
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function reportCleanupError(error: unknown) {
  console.error(`[native-video] cleanup failed: ${errorMessage(error)}`);
}
