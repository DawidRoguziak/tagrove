import { useEffect, useLayoutEffect, useRef } from "react";
import type { SelectedAsset } from "../../../types";

interface UseLightboxKeyboardShortcutsOptions {
  enabled?: boolean;
  selected: SelectedAsset | null;
  isFullscreen: boolean;
  onNavigatePrevious: () => void;
  onNavigateNext: () => void;
  onToggleFullscreen: () => Promise<void>;
  onResetZoom: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
}

export function useLightboxKeyboardShortcuts({
  enabled = true,
  selected,
  isFullscreen,
  onNavigatePrevious,
  onNavigateNext,
  onToggleFullscreen,
  onResetZoom,
  onZoomIn,
  onZoomOut
}: UseLightboxKeyboardShortcutsOptions) {
  const selectedKind = selected?.kind ?? null;
  const active = enabled && selectedKind !== null;
  const handlers = {
    selectedKind,
    isFullscreen,
    onNavigatePrevious,
    onNavigateNext,
    onToggleFullscreen,
    onResetZoom,
    onZoomIn,
    onZoomOut
  };
  const handlersRef = useRef(handlers);

  useLayoutEffect(() => {
    handlersRef.current = handlers;
  });

  useEffect(() => {
    if (!active) {
      return;
    }

    // An earlier keydown listener can reveal idle controls and commit a render.
    // Keep this listener registered so that same event still reaches the shortcut.
    const onKeyDown = (event: KeyboardEvent) => {
      const {
        selectedKind,
        isFullscreen,
        onNavigatePrevious,
        onNavigateNext,
        onToggleFullscreen,
        onResetZoom,
        onZoomIn,
        onZoomOut
      } = handlersRef.current;
      const target = event.target instanceof HTMLElement ? event.target : null;
      const isFormControl =
        target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || Boolean(target?.isContentEditable);

      if (selectedKind === "video" && isFullscreen && event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        if (!event.repeat) void onToggleFullscreen();
        return;
      }

      if (isFormControl) return;
      const isVideoPlayerTarget = Boolean(target?.closest("[data-lightbox-video-player]"));

      if (event.key === "f" || event.key === "F") {
        event.preventDefault();
        event.stopPropagation();
        void onToggleFullscreen();
        return;
      }

      if (selectedKind === "video" && isVideoPlayerTarget) {
        return;
      }

      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onNavigatePrevious();
        return;
      }

      if (event.key === "ArrowRight") {
        event.preventDefault();
        onNavigateNext();
        return;
      }

      if (selectedKind === "video") {
        return;
      }

      if (event.key === "0") {
        event.preventDefault();
        onResetZoom();
        return;
      }

      if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        onZoomIn();
        return;
      }

      if (event.key === "-") {
        event.preventDefault();
        onZoomOut();
      }
    };

    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [active]);
}
