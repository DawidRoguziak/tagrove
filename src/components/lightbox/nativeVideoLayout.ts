import type { VideoBounds } from "./mpvVideoTypes";

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
    height: Math.max(0, bottom - y)
  };
}
