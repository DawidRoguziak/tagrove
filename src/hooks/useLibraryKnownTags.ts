import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { listTags } from "../api";
const PAGE_SIZE = 200;

export function useLibraryKnownTags() {
  const [knownTags, publish] = useState<string[]>([]);
  const identity = useRef(0);
  const generation = useRef(0);
  const running = useRef<Promise<string[]> | null>(null);
  const mounted = useRef(true);
  const refreshKnownTags = useCallback(() => {
    generation.current++;
    if (running.current) return running.current;
    const capturedIdentity = identity.current;
    const run = async () => {
      while (mounted.current && capturedIdentity === identity.current) {
        const requested = generation.current;
        const items: string[] = [];
        let total = Infinity;
        while (items.length < total && requested === generation.current && mounted.current) {
          const page = await listTags({ query: "", offset: items.length, limit: PAGE_SIZE });
          if (requested !== generation.current || !mounted.current) break;
          items.push(...page.items);
          total = page.total;
          if (page.items.length === 0) break;
        }
        if (!mounted.current || capturedIdentity !== identity.current) return [];
        if (requested !== generation.current) continue;
        publish(items);
        return items;
      }
      return [];
    };
    const promise = run().finally(() => { if (running.current === promise) running.current = null; });
    running.current = promise;
    return promise;
  }, []);
  const setKnownTags = useCallback<Dispatch<SetStateAction<string[]>>>(value => {
    generation.current++;
    identity.current++;
    running.current = null;
    publish(value);
  }, []);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; generation.current++; };
  }, []);
  const hydrateKnownTags = useCallback(async () => {
    try { await refreshKnownTags(); } catch { /* Retain the previous vocabulary on failure. */ }
  }, [refreshKnownTags]);
  return { knownTags, setKnownTags, refreshKnownTags, hydrateKnownTags };
}
