import { createSignal, onCleanup } from "solid-js";
import { listTags } from "../../platform/tauri/tags";
const PAGE_SIZE = 200;

export function createKnownTags() {
  const [knownTags, publish] = createSignal<string[]>([]);
  let snapshot: string[] = [];
  let dirty = true;
  let identity = 0;
  let generation = 0;
  let running: Promise<string[]> | null = null;
  let mounted = true;
  const refreshKnownTags = () => {
    dirty = true;
    generation++;
    if (running) return running;
    const capturedIdentity = identity;
    const run = async () => {
      while (mounted && capturedIdentity === identity) {
        const requested = generation;
        const items: string[] = [];
        let total = Infinity;
        try {
          while (items.length < total && requested === generation && mounted) {
            const page = await listTags({ query: "", offset: items.length, limit: PAGE_SIZE });
            if (requested !== generation || !mounted) break;
            items.push(...page.items);
            total = page.total;
            if (page.items.length === 0) break;
          }
        } catch (error) {
          if (!mounted || capturedIdentity !== identity) return [];
          // A newer mutation still needs its pass even if the superseded read failed.
          if (requested !== generation) continue;
          throw error;
        }
        if (!mounted || capturedIdentity !== identity) return [];
        if (requested !== generation) continue;
        const previous = snapshot;
        const next =
          previous.length === items.length && previous.every((tag, index) => tag === items[index])
            ? previous
            : items;
        snapshot = next;
        dirty = false;
        publish(next);
        return next;
      }
      return [];
    };
    const promise = run().finally(() => {
      if (running === promise) running = null;
    });
    running = promise;
    return promise;
  };
  const refreshKnownTagsAfterMutation = (vocabularyChanged: boolean) => {
    if (vocabularyChanged) return refreshKnownTags();
    if (running) return running;
    if (dirty) return refreshKnownTags();
    return Promise.resolve(snapshot);
  };
  const reset = () => {
    generation++;
    identity++;
    running = null;
    dirty = true;
    snapshot = [];
    publish(snapshot);
  };
  onCleanup(() => {
    mounted = false;
    reset();
  });
  const hydrateKnownTags = async () => {
    try {
      await refreshKnownTags();
    } catch {
      /* Retain the previous vocabulary on failure. */
    }
  };
  return {
    get knownTags() {
      return knownTags();
    },
    reset,
    refreshKnownTags,
    refreshKnownTagsAfterMutation,
    hydrateKnownTags
  };
}
