import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { getAssetSummariesByIds } from "../../platform/tauri/assets";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import type { MetadataCommit } from "../../library/metadata/createMutations";

/** Selected records survive gallery eviction, but never outlive their selection. */
export function createSelectedSummaries(options: {
  ids: () => ReadonlySet<number>;
  cached: () => AssetSummary[];
  epoch: () => number;
  subscribe: (listener: (commit: MetadataCommit) => void) => () => void;
}) {
  let records = new Map<number, AssetSummary>();
  let patches = new Map<number, Partial<AssetSummary>>();
  let deleted = new Set<number>();
  let failures = new Set<number>();
  let generation = 0;
  let identity = options.epoch();
  let running = 0;
  let queue: number[][] = [];
  let disposed = false;
  const [version, publish] = createSignal(0);
  const [retry, setRetry] = createSignal(0);
  const changed = () => publish((v) => v + 1);
  function pump() {
    while (!disposed && running < 4 && queue.length) {
      const batch = queue.shift()!;
      const request = generation;
      const epoch = options.epoch();
      running++;
      void getAssetSummariesByIds(batch)
        .then((items) => {
          if (disposed || request !== generation || epoch !== options.epoch()) return;
          const found = new Set(items.map((item) => item.id));
          for (const item of items) {
            if (options.ids().has(item.id) && !deleted.has(item.id))
              records.set(item.id, { ...item, ...patches.get(item.id) });
          }
          for (const id of batch) if (!found.has(id)) failures.add(id);
          changed();
        })
        .catch(() => {
          if (disposed || request !== generation || epoch !== options.epoch()) return;
          for (const id of batch) failures.add(id);
          changed();
        })
        .finally(() => {
          running--;
          pump();
        });
    }
  }
  createEffect(
    () => ({ ids: options.ids(), epoch: options.epoch(), retry: retry() }),
    ({ ids, epoch }) => {
      generation++;
      records = new Map(identity === epoch ? [...records].filter(([id]) => ids.has(id)) : []);
      patches = new Map(identity === epoch ? [...patches].filter(([id]) => ids.has(id)) : []);
      deleted = new Set(identity === epoch ? [...deleted].filter((id) => ids.has(id)) : []);
      identity = epoch;
      for (const item of options.cached())
        if (ids.has(item.id) && !records.has(item.id) && !deleted.has(item.id))
          records.set(item.id, { ...item, ...patches.get(item.id) });
      failures = new Set();
      const missing = [...ids].filter((id) => !records.has(id));
      queue = [];
      for (let i = 0; i < missing.length; i += 256) queue.push(missing.slice(i, i + 256));
      changed();
      pump();
    }
  );
  const unsubscribe = options.subscribe((commit) => {
    if (commit.type === "deleted")
      for (const id of commit.ids) {
        if (!options.ids().has(id)) continue;
        records.delete(id);
        deleted.add(id);
        failures.add(id);
      }
    else
      for (const [id, patch] of commit.patches) {
        if (!options.ids().has(id)) continue;
        const { tags: _tags, ...summary } = patch;
        patches.set(id, { ...patches.get(id), ...summary });
        const previous = records.get(id);
        if (previous) {
          records.set(id, { ...previous, ...summary });
        }
      }
    changed();
  });
  onCleanup(() => {
    disposed = true;
    generation++;
    queue = [];
    unsubscribe();
  });
  const selected = createMemo(() => {
    version();
    return [...options.ids()].flatMap((id) => {
      const item = records.get(id);
      return item ? [item] : [];
    });
  });
  return {
    get selected() {
      return selected();
    },
    get ready() {
      return selected().length === options.ids().size;
    },
    get failed() {
      version();
      return failures.size > 0;
    },
    retry: () => setRetry((v) => v + 1)
  };
}
