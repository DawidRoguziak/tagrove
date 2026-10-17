import { onCleanup } from "solid-js";
import { getAssetDetails } from "../../platform/tauri/assets";
import type { AssetDetails } from "../../platform/tauri/contracts/assets";
import type { AssetTagGenerationToken, AssetTagStateController } from "./createAssetTagState";

export function createDetailsCache(tags: AssetTagStateController) {
  const entries = new Map<number, { token: AssetTagGenerationToken; details: AssetDetails }>();
  const pending = new Map<number, Promise<AssetDetails | null>>();
  let generation = 0;
  let disposed = false;
  function clear() {
    generation++;
    entries.clear();
    pending.clear();
  }
  onCleanup(() => {
    disposed = true;
    clear();
  });
  function get(id: number) {
    const entry = entries.get(id);
    if (!entry) return null;
    if (!tags.isCurrent(entry.token)) {
      entries.delete(id);
      return null;
    }
    entries.delete(id);
    entries.set(id, entry);
    return entry.details;
  }
  function put(details: AssetDetails, expectedGeneration: number) {
    if (disposed || expectedGeneration !== generation) return;
    const token = tags.captureGeneration(details.id);
    if (!tags.isCurrent(token)) return;
    entries.delete(details.id);
    entries.set(details.id, { token, details });
    while (entries.size > 256) {
      const oldest = entries.keys().next();
      if (oldest.done) break;
      entries.delete(oldest.value);
    }
  }
  function load(id: number): Promise<AssetDetails | null> {
    if (disposed) return Promise.resolve(null);
    const cached = get(id);
    if (cached) return Promise.resolve(cached);
    const existing = pending.get(id);
    if (existing) return existing;
    const epoch = generation;
    const release = tags.pin(id);
    const token = tags.captureGeneration(id);
    const request = getAssetDetails(id)
      .then((details) => {
        if (
          disposed ||
          epoch !== generation ||
          !details ||
          !tags.publishDetails(id, details.tags, token)
        )
          return null;
        const canonical = { ...details, tags: tags.get(id)?.tags ?? details.tags };
        put(canonical, epoch);
        return canonical;
      })
      .finally(() => {
        release();
        if (pending.get(id) === request) pending.delete(id);
      });
    pending.set(id, request);
    return request;
  }
  return {
    get generation() {
      return generation;
    },
    get,
    put,
    load,
    clear,
    remove: (id: number) => entries.delete(id)
  };
}
