import { useContainer, useMediaAttach } from "@videojs/react";
import { useEffect, useRef, useState } from "react";
import { closeVideo, controlVideo, openVideo, setVideoBounds } from "../../api";
import { MpvMediaAdapter } from "./MpvMediaAdapter";
import { measureNativeVideoBounds } from "./nativeVideoLayout";
import type { MpvVideoEvent } from "./mpvVideoTypes";

interface MpvMediaComponentProps {
  assetId?: number;
  generation?: number;
  adapter?: MpvMediaAdapter;
  onAdapter?: (adapter: MpvMediaAdapter | null) => void;
  onLoadedMetadata?: (dimensions: { width: number; height: number }) => void;
  onFullscreenChange?: (fullscreen: boolean) => void;
  onError?: (message: string) => void;
}

export function MpvMediaComponent({
  assetId,
  generation = 0,
  adapter: providedAdapter,
  onAdapter,
  onLoadedMetadata,
  onFullscreenChange,
  onError
}: MpvMediaComponentProps) {
  const attachMedia = useMediaAttach();
  const container = useContainer();
  const sessionRef = useRef<number | null>(null);
  const callbacksRef = useRef({ onLoadedMetadata, onFullscreenChange, onError });
  callbacksRef.current = { onLoadedMetadata, onFullscreenChange, onError };
  const [adapter] = useState(
    () =>
      providedAdapter ??
      new MpvMediaAdapter(`asset:${assetId}`, {
        play: () => controlCurrent(sessionRef, { type: "play" }),
        pause: () => controlCurrent(sessionRef, { type: "pause" }),
        seek: (time) => controlCurrent(sessionRef, { type: "seek", time }),
        setVolume: (volume) => controlCurrent(sessionRef, { type: "setVolume", volume }),
        setMuted: (muted) => controlCurrent(sessionRef, { type: "setMuted", muted }),
        setRate: (rate) => controlCurrent(sessionRef, { type: "setRate", rate }),
        setFullscreen: (fullscreen) =>
          controlCurrent(sessionRef, { type: "setFullscreen", fullscreen })
      })
  );

  useEffect(() => {
    attachMedia?.(adapter);
    onAdapter?.(adapter);
    return () => {
      onAdapter?.(null);
      attachMedia?.((current) => (current === adapter ? null : current));
    };
  }, [adapter, attachMedia, onAdapter]);

  useEffect(() => {
    if (assetId === undefined || !container) return;
    let cancelled = false;
    let openedSession: number | null = null;
    let boundsFrame: number | null = null;
    let boundsQueue = Promise.resolve();
    const pendingEvents: MpvVideoEvent[] = [];
    const reportError = (phase: string, error: unknown) => {
      const message = errorMessage(error);
      console.error(
        `[native-video] ${phase} failed for asset ${assetId}, generation ${generation}: ${message}`
      );
      callbacksRef.current.onError?.(message);
    };
    const publishCurrentBounds = (sessionId: number) => {
      const bounds = measureNativeVideoBounds(container);
      const operation = boundsQueue.catch(() => {}).then(async () => {
        if (cancelled || sessionRef.current !== sessionId) return;
        await setVideoBounds(sessionId, bounds);
      });
      boundsQueue = operation;
      return operation;
    };
    const scheduleBounds = () => {
      if (boundsFrame !== null) return;
      boundsFrame = requestAnimationFrame(() => {
        boundsFrame = null;
        const sessionId = sessionRef.current;
        if (sessionId !== null) {
          void publishCurrentBounds(sessionId).catch((error) => {
            if (!cancelled && sessionRef.current === sessionId) {
              console.error(
                `[native-video] bounds update failed for session ${sessionId}: ${errorMessage(error)}`
              );
            }
          });
        }
      });
    };
    const applyEvent = (event: MpvVideoEvent) => {
      if (cancelled) return;
      if (openedSession === null) {
        pendingEvents.push(event);
        return;
      }
      if (event.session_id !== openedSession) return;
      adapter.applyBackendEvent(event);
      if (event.type === "metadata") {
        callbacksRef.current.onLoadedMetadata?.({ width: event.width, height: event.height });
      } else if (event.type === "fullscreen") {
        callbacksRef.current.onFullscreenChange?.(event.fullscreen);
        scheduleBounds();
      } else if (event.type === "error") {
        reportError("playback", event.message);
      }
    };

    void (async () => {
      try {
        const sessionId = await openVideo(
          assetId,
          generation,
          measureNativeVideoBounds(container),
          applyEvent
        );
        if (cancelled) {
          await closeVideo(sessionId).catch(() => {});
          return;
        }
        openedSession = sessionId;
        sessionRef.current = sessionId;
        for (const event of pendingEvents) applyEvent(event);
        await publishCurrentBounds(sessionId);
      } catch (error) {
        if (cancelled) return;
        const sessionId = openedSession;
        if (sessionId !== null) {
          openedSession = null;
          if (sessionRef.current === sessionId) sessionRef.current = null;
          await closeVideo(sessionId).catch(() => {});
        }
        reportError("open", error);
      }
    })();

    const observer = new ResizeObserver(scheduleBounds);
    observer.observe(container);
    window.addEventListener("resize", scheduleBounds);

    return () => {
      cancelled = true;
      if (boundsFrame !== null) cancelAnimationFrame(boundsFrame);
      observer.disconnect();
      window.removeEventListener("resize", scheduleBounds);
      const sessionId = openedSession;
      if (sessionRef.current === sessionId) sessionRef.current = null;
      if (sessionId !== null) void closeVideo(sessionId).catch(() => {});
    };
  }, [adapter, assetId, container, generation]);

  return null;
}

async function controlCurrent(
  sessionRef: { current: number | null },
  command: Parameters<typeof controlVideo>[1]
) {
  const sessionId = sessionRef.current;
  if (sessionId !== null) await controlVideo(sessionId, command);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : String(error);
}
