import { createEffect, createSignal, untrack } from "solid-js";
import type { ThumbnailStore } from "./thumbnailStore";

/** A tile owns one subscription, including when a virtual slot changes identity. */
export function createAssetThumbnail(store: ThumbnailStore, assetId: () => number) {
  const read = (id: number) => ({ id, path: store.getPath(id), rendering: store.isRendering(id) });
  const [snapshot, publish] = createSignal(untrack(() => read(assetId())));
  createEffect(assetId, (id) => {
    const update = () => {
      const next = read(id);
      publish((previous) =>
        previous.id === next.id &&
        previous.path === next.path &&
        previous.rendering === next.rendering
          ? previous
          : next
      );
    };
    const unsubscribe = store.subscribe(id, update);
    update();
    return unsubscribe;
  });
  // The effect owns subscription cleanup on identity changes and owner disposal.
  return () => {
    const current = snapshot();
    return current.id === assetId() ? current : read(assetId());
  };
}
