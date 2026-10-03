import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { listTags } from "../api";
const PAGE_SIZE = 200;

export function useLibraryKnownTags() {
  const [knownTags, publish] = useState<string[]>([]);
  const snapshot = useRef(knownTags);
  const dirty = useRef(true);
  const identity = useRef(0);
  const generation = useRef(0);
  const running = useRef<Promise<string[]> | null>(null);
  const mounted = useRef(true);
  const refreshKnownTags = useCallback(() => {
    dirty.current = true;
    generation.current++;
    if (running.current) return running.current;
    const capturedIdentity = identity.current;
    const run = async () => {
      while (mounted.current && capturedIdentity === identity.current) {
        const requested = generation.current;
        const items: string[] = [];
        let total = Infinity;
        try {
          while (items.length < total && requested === generation.current && mounted.current) {
            const page = await listTags({ query: "", offset: items.length, limit: PAGE_SIZE });
            if (requested !== generation.current || !mounted.current) break;
            items.push(...page.items);
            total = page.total;
            if (page.items.length === 0) break;
          }
        } catch (error) {
          if (!mounted.current || capturedIdentity !== identity.current) return [];
          // A newer mutation still needs its pass even if the superseded read failed.
          if (requested !== generation.current) continue;
          throw error;
        }
        if (!mounted.current || capturedIdentity !== identity.current) return [];
        if (requested !== generation.current) continue;
        const previous = snapshot.current;
        const next = previous.length === items.length && previous.every((tag, index) => tag === items[index])
          ? previous : items;
        snapshot.current = next;
        dirty.current = false;
        publish(next);
        return next;
      }
      return [];
    };
    const promise = run().finally(() => { if (running.current === promise) running.current = null; });
    running.current = promise;
    return promise;
  }, []);
  const refreshKnownTagsAfterMutation = useCallback((vocabularyChanged: boolean) => {
    if (vocabularyChanged) return refreshKnownTags();
    if (running.current) return running.current;
    if (dirty.current) return refreshKnownTags();
    return Promise.resolve(snapshot.current);
  }, [refreshKnownTags]);
  const setKnownTags = useCallback<Dispatch<SetStateAction<string[]>>>(value => {
    generation.current++;
    identity.current++;
    running.current = null;
    dirty.current = true;
    const next = typeof value === "function" ? value(snapshot.current) : value;
    snapshot.current = next;
    publish(next);
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      generation.current++;
      identity.current++;
      running.current = null;
      dirty.current = true;
    };
  }, []);
  const hydrateKnownTags = useCallback(async () => {
    try { await refreshKnownTags(); } catch { /* Retain the previous vocabulary on failure. */ }
  }, [refreshKnownTags]);
  return { knownTags, setKnownTags, refreshKnownTags, refreshKnownTagsAfterMutation, hydrateKnownTags };
}
