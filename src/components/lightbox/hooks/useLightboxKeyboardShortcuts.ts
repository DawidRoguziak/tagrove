import { useEffect } from "react";
import type { SelectedAsset } from "../../../types";

interface UseLightboxKeyboardShortcutsOptions {
  selected: SelectedAsset | null;
  onClose: () => void;
  onNavigatePrevious: () => void;
  onNavigateNext: () => void;
  onToggleFullscreen: () => Promise<void>;
  onResetZoom: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
}

export function useLightboxKeyboardShortcuts({
  selected,
  onClose,
  onNavigatePrevious,
  onNavigateNext,
  onToggleFullscreen,
  onResetZoom,
  onZoomIn,
  onZoomOut
}: UseLightboxKeyboardShortcutsOptions) {
  const selectedKind = selected?.kind ?? null;

  useEffect(() => {
    if (selectedKind === null) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target instanceof HTMLElement ? event.target : null;
      const isFormControl =
        target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || Boolean(target?.isContentEditable);

      if (event.key === "Escape") {
        if (document.fullscreenElement) {
          return;
        }

        onClose();
        return;
      }

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
  }, [onClose, onNavigateNext, onNavigatePrevious, onResetZoom, onToggleFullscreen, onZoomIn, onZoomOut, selectedKind]);
}
