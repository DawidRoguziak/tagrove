import { useCallback, useEffect, useRef, useState } from "react";
import {
  beginVideoOpen,
  cancelVideoOpen,
  closeVideo,
  controlVideo,
  openVideo,
  setVideoBounds,
  setVideoControlLabels
} from "../../api";
import { measureNativeVideoBounds } from "./nativeVideoLayout";
import type {
  MpvVideoEvent,
  NativeVideoControlLabels,
  PlaybackSnapshot,
  VideoBounds
} from "./mpvVideoTypes";

interface SessionOptions {
  assetId: number;
  generation: number;
  container: HTMLElement | null;
  controlLabels: NativeVideoControlLabels;
  onLoadedMetadata: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange: (fullscreen: boolean) => void;
  onError: () => void;
  onPointerActivity?: () => void;
  onNativeBounds?: (bounds: VideoBounds) => void;
}

interface Session {
  id: number;
}

export function useNativeVideoSession(options: SessionOptions) {
  const { assetId, generation, container, controlLabels } = options;
  const callbacks = useRef(options);
  callbacks.current = options;
  const session = useRef<Session | null>(null);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [snapshot, setSnapshot] = useState<PlaybackSnapshot | null>(null);
  const [controlError, setControlError] = useState<string | null>(null);

  useEffect(() => {
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
    callbacks.current.onFullscreenChange(false);
    setSnapshot(null);
    setControlError(null);

    const fail = (error: unknown) => {
      if (cancelled) return;
      console.error(`[native-video] asset ${assetId}: ${errorMessage(error)}`);
      cancelled = true;
      if (current) {
        if (session.current === current) session.current = null;
        void closeVideo(current.id).catch(reportCleanupError);
      }
      if (requestId !== null) void cancelVideoOpen(requestId).catch(reportCleanupError);
      setSessionId(null);
      callbacks.current.onFullscreenChange(false);
      callbacks.current.onError();
    };
    const publishBounds = async () => {
      if (sendingBounds || !current || cancelled) return;
      sendingBounds = true;
      try {
        while (pendingBounds && !cancelled) {
          const bounds = pendingBounds;
          pendingBounds = null;
          await setVideoBounds(current.id, bounds);
          if (!cancelled) callbacks.current.onNativeBounds?.(bounds);
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
            callbacks.current.onLoadedMetadata({ width: event.width, height: event.height });
          }
          break;
        case "snapshot":
          setSnapshot(event.state);
          break;
        case "fullscreen":
          setFullscreen(event.fullscreen);
          callbacks.current.onFullscreenChange(event.fullscreen);
          scheduleBounds();
          break;
        case "pointerActivity":
          callbacks.current.onPointerActivity?.();
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
          callbacks.current.controlLabels,
          applyEvent
        );
        if (cancelled) {
          await closeVideo(id);
          return;
        }
        current = { id };
        session.current = current;
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
      if (session.current === current) session.current = null;
      if (requestId !== null) void cancelVideoOpen(requestId).catch(reportCleanupError);
      if (current) void closeVideo(current.id).catch(reportCleanupError);
    };
  }, [assetId, generation, container]);

  useEffect(() => {
    if (sessionId === null) return;
    let cancelled = false;
    void setVideoControlLabels(sessionId, controlLabels).catch((error) => {
      if (!cancelled) setControlError(errorMessage(error));
    });
    return () => {
      cancelled = true;
    };
  }, [controlLabels, sessionId]);

  const fullscreenQueue = useRef(Promise.resolve());
  const toggleFullscreen = useCallback(() => {
    const target = session.current;
    const operation = fullscreenQueue.current.then(async () => {
      if (!target || session.current !== target) return;
      try {
        await controlVideo(target.id, { type: "toggleFullscreen" });
      } catch (error) {
        if (session.current === target) setControlError(errorMessage(error));
      }
    });
    fullscreenQueue.current = operation;
    return operation;
  }, []);

  return {
    sessionId,
    fullscreen,
    snapshot,
    controlError,
    dismissControlError: () => setControlError(null),
    toggleFullscreen
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function reportCleanupError(error: unknown) {
  console.error(`[native-video] cleanup failed: ${errorMessage(error)}`);
}
