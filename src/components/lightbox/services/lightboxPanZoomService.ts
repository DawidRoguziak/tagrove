export interface MediaDimensions {
  width: number;
  height: number;
}

export const MIN_ZOOM_FACTOR = 1;
export const MAX_ZOOM_FACTOR = 12;
export const ZOOM_MULTIPLIER = 1.25;

const DOUBLE_CLICK_BASE_ZOOM_FACTOR = 2;
const ZOOM_EPSILON = 0.001;

export function calculateFitScale(intrinsicSize: MediaDimensions, mediaSize: MediaDimensions) {
  if (!intrinsicSize.width || !intrinsicSize.height || !mediaSize.width || !mediaSize.height) {
    return 1;
  }

  return Math.min(mediaSize.width / intrinsicSize.width, mediaSize.height / intrinsicSize.height);
}

export function calculateFittedMediaSize(intrinsicSize: MediaDimensions, mediaSize: MediaDimensions) {
  if (!intrinsicSize.width || !intrinsicSize.height || !mediaSize.width || !mediaSize.height) {
    return {
      fitScale: 1,
      width: 0,
      height: 0
    };
  }

  const fitScale = calculateFitScale(intrinsicSize, mediaSize);

  return {
    fitScale,
    width: intrinsicSize.width * fitScale,
    height: intrinsicSize.height * fitScale
  };
}

export function clampZoomFactor(nextZoomFactor: number) {
  return Math.max(MIN_ZOOM_FACTOR, Math.min(MAX_ZOOM_FACTOR, nextZoomFactor));
}

export function getNextZoomFactor(currentZoomFactor: number, direction: "in" | "out") {
  return clampZoomFactor(
    direction === "in" ? currentZoomFactor * ZOOM_MULTIPLIER : currentZoomFactor / ZOOM_MULTIPLIER
  );
}

export function getDoubleClickZoomFactor(fitScale: number) {
  const pixelPerfectZoomFactor = fitScale > 0 ? 1 / fitScale : DOUBLE_CLICK_BASE_ZOOM_FACTOR;
  return clampZoomFactor(Math.max(DOUBLE_CLICK_BASE_ZOOM_FACTOR, pixelPerfectZoomFactor));
}

export function clampPan(
  next: { x: number; y: number },
  nextZoomFactor: number,
  fittedMediaSize: MediaDimensions,
  mediaSize: MediaDimensions
) {
  if (nextZoomFactor <= MIN_ZOOM_FACTOR + ZOOM_EPSILON) {
    return { x: 0, y: 0 };
  }

  const overflowX = Math.max(0, fittedMediaSize.width * nextZoomFactor - mediaSize.width);
  const overflowY = Math.max(0, fittedMediaSize.height * nextZoomFactor - mediaSize.height);
  const maxX = overflowX / 2;
  const maxY = overflowY / 2;

  return {
    x: Math.max(-maxX, Math.min(maxX, next.x)),
    y: Math.max(-maxY, Math.min(maxY, next.y))
  };
}

export function buildImageTransform(pan: { x: number; y: number }, zoomFactor: number) {
  return `translate3d(${pan.x}px, ${pan.y}px, 0) scale(${zoomFactor})`;
}
