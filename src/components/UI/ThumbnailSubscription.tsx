import { createContext, useCallback, useContext, useSyncExternalStore } from "react";
import type { ThumbnailStore } from "../../hooks/services/thumbnailStore";
import { toMediaSrc } from "../../api";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "./ThumbnailImage";
export const ThumbnailContext = createContext<ThumbnailStore | null>(null);
export function useThumbnailSubscription(
  id: number,
  fallbackPath?: string,
  fallbackRendering = false
) {
  const store = useContext(ThumbnailContext);
  const subscribe = useCallback(
    (listener: () => void) => store?.subscribe(id, listener) ?? (() => {}),
    [store, id]
  );
  const snapshot = useCallback(() => store?.getVersion(id) ?? 0, [store, id]);
  useSyncExternalStore(subscribe, snapshot, snapshot);
  return {
    path: store ? store.getPath(id) : fallbackPath,
    rendering: store ? store.isRendering(id) : fallbackRendering
  };
}
export function ThumbnailSubscription({
  id,
  path,
  rendering
}: {
  id: number;
  path?: string;
  rendering?: boolean;
}) {
  const current = useThumbnailSubscription(id, path, rendering);
  return (
    <>
      <ThumbnailImage
        src={current.path ? toMediaSrc(current.path) : TRANSPARENT_THUMBNAIL_SRC}
        alt=""
        className="h-full w-full object-cover"
        draggable={false}
      />
      {current.rendering && !current.path ? (
        <span
          className="absolute inset-0 grid place-items-center bg-base-100/55"
          aria-hidden="true"
        >
          <span className="h-4 w-4 animate-spin rounded-full border-2 border-base-content/25 border-t-primary" />
        </span>
      ) : null}
    </>
  );
}
