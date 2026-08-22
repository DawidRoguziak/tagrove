import { useEffect } from "react";
import type { SelectedAsset } from "../../../types";

interface UseLightboxKeyboardShortcutsOptions {
  enabled?: boolean;
  selected: SelectedAsset | null;
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
  onNavigatePrevious,
  onNavigateNext,
  onToggleFullscreen,
  onResetZoom,
  onZoomIn,
  onZoomOut
}: UseLightboxKeyboardShortcutsOptions) {
  const selectedKind = selected?.kind ?? null;

  useEffect(() => {
    if (!enabled || selectedKind === null) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target instanceof HTMLElement ? event.target : null;
      const isFormControl =
        target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || Boolean(target?.isContentEditable);

      if (isFormControl) {
        return;
      }

      const isVideoPlayerTarget = Boolean(target?.closest("[data-lightbox-video-player]"));
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

      if (event.key === "f" || event.key === "F") {
        event.preventDefault();
        void onToggleFullscreen();
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

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled, onNavigateNext, onNavigatePrevious, onResetZoom, onToggleFullscreen, onZoomIn, onZoomOut, selectedKind]);
}
