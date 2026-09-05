import { useEffect } from "react";
import type { RefObject } from "react";

interface UseLightboxViewportSizeOptions {
  selectedId: number | null;
  fullscreen: boolean;
  mediaViewportRef: RefObject<HTMLDivElement | null>;
  onSizeChange: (next: { width: number; height: number }) => void;
}

export function useLightboxViewportSize({
  selectedId,
  fullscreen,
  mediaViewportRef,
  onSizeChange
}: UseLightboxViewportSizeOptions) {
  useEffect(() => {
    if (selectedId === null) {
      return;
    }

    const node = mediaViewportRef.current;
    if (!node) {
      return;
    }

    const updateSize = () => {
      onSizeChange({
        width: node.clientWidth,
        height: node.clientHeight
      });
    };

    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(node);
    return () => observer.disconnect();
  }, [mediaViewportRef, onSizeChange, selectedId, fullscreen]);
}
