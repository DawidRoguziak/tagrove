import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getAssetSummariesByIds } from "../../../api";
import type { AssetSummary } from "../../../types";

/** Selection metadata has its own lifetime; gallery eviction never removes it. */
export function useSelectedSummaries(
  ids: ReadonlySet<number>,
  cached: AssetSummary[],
  epoch: number
) {
  const [records, setRecords] = useState(new Map<number, AssetSummary>());
  const [failed, setFailed] = useState(new Set<number>());
  const [retry, setRetry] = useState(0);
  const cachedRef = useRef(cached);
  cachedRef.current = cached;
  const idsRef = useRef(ids);
  idsRef.current = ids;
  const recordsRef = useRef(records);
  recordsRef.current = records;
  const running = useRef(0);
  const queue = useRef<number[][]>([]);
  const identityRef = useRef(epoch);
  const generation = useRef(0);
  const pumpRef = useRef<() => void>(() => {});
  const pump = useCallback(() => {
    while (running.current < 4 && queue.current.length > 0) {
      const batch = queue.current.shift()!;
      const request = generation.current;
      running.current++;
      void getAssetSummariesByIds(batch)
        .then((items) => {
          if (generation.current !== request) return;
          const found = new Set(items.map((item) => item.id));
          setRecords((previous) => {
            const next = new Map(previous);
            for (const item of items) if (idsRef.current.has(item.id)) next.set(item.id, item);
            return next;
          });
          setFailed((previous) => new Set([...previous, ...batch.filter((id) => !found.has(id))]));
        })
        .catch(() => {
          if (generation.current === request)
            setFailed((previous) => new Set([...previous, ...batch]));
        })
        .finally(() => {
          running.current--;
          pumpRef.current();
        });
    }
  }, []);
  pumpRef.current = pump;
  // biome-ignore lint/correctness/useExhaustiveDependencies: retry and library identity deliberately restart hydration.
  useEffect(() => {
    generation.current++;
    const next = new Map<number, AssetSummary>();
    if (identityRef.current === epoch) {
      for (const [id, item] of recordsRef.current) if (ids.has(id)) next.set(id, item);
    }
    identityRef.current = epoch;
    for (const item of cachedRef.current)
      if (ids.has(item.id) && !next.has(item.id)) next.set(item.id, item);
    setRecords(next);
    setFailed(new Set());
    const missing = [...ids].filter((id) => !next.has(id));
    queue.current = [];
    for (let offset = 0; offset < missing.length; offset += 256)
      queue.current.push(missing.slice(offset, offset + 256));
    pump();
    return () => {
      generation.current++;
      queue.current = [];
    };
  }, [ids, retry, epoch, pump]);
  const selected = useMemo(
    () =>
      [...ids].flatMap((id) => {
        const item = records.get(id);
        return item ? [item] : [];
      }),
    [ids, records]
  );
  const patch = useCallback((update: (asset: AssetSummary) => AssetSummary) => {
    setRecords((previous) => new Map([...previous].map(([id, item]) => [id, update(item)])));
  }, []);
  return {
    selected,
    ready: selected.length === ids.size,
    failed: failed.size > 0,
    retry: useCallback(() => setRetry((value) => value + 1), []),
    patch
  };
}
