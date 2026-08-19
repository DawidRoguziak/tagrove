import { useCallback, useRef, useState } from "react";
import { listTags } from "../api";

const PAGE_SIZE = 200;

export function useLibraryKnownTags() {
  const [knownTags, setKnownTags] = useState<string[]>([]);
  const requestGenerationRef = useRef(0);

  const refreshKnownTags = useCallback(async () => {
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    const items: string[] = [];
    let offset = 0;
    let total = Number.POSITIVE_INFINITY;
    while (offset < total) {
      const page = await listTags({ query: "", offset, limit: PAGE_SIZE });
      items.push(...page.items);
      total = page.total;
      if (page.items.length === 0) break;
      offset += page.items.length;
    }
    if (requestGenerationRef.current === generation) setKnownTags(items);
    return items;
  }, []);

  const hydrateKnownTags = useCallback(async () => {
    try {
      await refreshKnownTags();
    } catch {
      // A failed or stale hydration must not discard a valid known-tag snapshot.
    }
  }, [refreshKnownTags]);

  return { knownTags, setKnownTags, refreshKnownTags, hydrateKnownTags };
}
