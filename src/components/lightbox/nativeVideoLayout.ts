import type { VideoBounds } from "./mpvVideoTypes";
import type { MediaDimensions } from "./services/lightboxPanZoomService";

export const NATIVE_VIDEO_CONTROL_STRIP_HEIGHT = 72;

export function getNativeVideoFrameViewport(
  viewport: MediaDimensions
): MediaDimensions {
  return {
    width: viewport.width,
    height: Math.max(0, viewport.height - NATIVE_VIDEO_CONTROL_STRIP_HEIGHT)
  };
}

export function getNativeVideoPlayerSize(
  frame: MediaDimensions
): MediaDimensions {
  if (!frame.width || !frame.height) return { width: 0, height: 0 };
  return {
    width: frame.width,
    height: frame.height + NATIVE_VIDEO_CONTROL_STRIP_HEIGHT
  };
}

export function measureNativeVideoBounds(element: HTMLElement): VideoBounds {
  const rect = element.getBoundingClientRect();
  const x = Math.max(0, Math.round(rect.left));
  const y = Math.max(0, Math.round(rect.top));
  const right = Math.max(x, Math.round(rect.right));
  const bottom = Math.max(y, Math.round(rect.bottom));
  return {
    x,
    y,
    width: right - x,
    height: Math.max(0, bottom - y - NATIVE_VIDEO_CONTROL_STRIP_HEIGHT)
  };
}
