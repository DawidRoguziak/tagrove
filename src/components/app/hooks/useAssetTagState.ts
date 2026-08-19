import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { normalizeTags } from "../../../utils/media";

export interface AssetTagGenerationToken {
  assetId: number;
  epoch: number;
  generation: number;
}

export type AssetTagMutationToken = AssetTagGenerationToken;

export interface AuthoritativeAssetTags {
  known: true;
  tags: string[];
  generation: number;
}

interface StoredAssetTags {
  tags: string[];
  generation: number;
}

interface AssetClock {
  generation: number;
  activeMutation: number | null;
  deleted: boolean;
}

export interface AssetTagStateController {
  revision: number;
  epoch: number;
  get: (assetId: number) => AuthoritativeAssetTags | null;
  captureGeneration: (assetId: number) => AssetTagGenerationToken;
  publishDetails: (assetId: number, tags: string[], token: AssetTagGenerationToken) => boolean;
  beginMutation: (assetId: number) => AssetTagMutationToken | null;
  settleMutation: (token: AssetTagMutationToken, tags?: string[]) => boolean;
  runWithMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
  remove: (assetId: number, identityToken?: AssetTagGenerationToken) => boolean;
  reset: () => void;
}

function sameTags(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index]);
}

/** Shell-owned source of complete, canonical tag lists shared by every tag editor. */
export function useAssetTagState(): AssetTagStateController {
  const entriesRef = useRef(new Map<number, StoredAssetTags>());
  const clocksRef = useRef(new Map<number, AssetClock>());
  const epochRef = useRef(0);
  const activeMutationTokensRef = useRef(new Set<string>());
  const activeMutationDrainWaitersRef = useRef(new Set<() => void>());
  const barrierRequestCountRef = useRef(0);
  const barrierTailRef = useRef<Promise<void>>(Promise.resolve());
  const disposedRef = useRef(false);
  const [epoch, setEpoch] = useState(0);
  const [revision, setRevision] = useState(0);

  const getClock = useCallback((assetId: number): AssetClock => {
    let clock = clocksRef.current.get(assetId);
    if (!clock) {
      clock = { generation: 0, activeMutation: null, deleted: false };
      clocksRef.current.set(assetId, clock);
    }
    return clock;
  }, []);

  const get = useCallback((assetId: number): AuthoritativeAssetTags | null => {
    const entry = entriesRef.current.get(assetId);
    return entry ? { known: true, tags: entry.tags, generation: entry.generation } : null;
  }, []);

  const captureGeneration = useCallback((assetId: number): AssetTagGenerationToken => {
    const clock = getClock(assetId);
    return { assetId, epoch: epochRef.current, generation: clock.generation };
  }, [getClock]);

  const publishDetails = useCallback(
    (assetId: number, rawTags: string[], token: AssetTagGenerationToken): boolean => {
      const clock = getClock(assetId);
      if (
        token.assetId !== assetId ||
        token.epoch !== epochRef.current ||
        token.generation !== clock.generation ||
        clock.activeMutation !== null ||
        clock.deleted
      ) {
        return false;
      }
      const tags = normalizeTags(rawTags);
      const previous = entriesRef.current.get(assetId);
      if (previous && previous.generation === clock.generation && sameTags(previous.tags, tags)) {
        return true;
      }
      entriesRef.current.set(assetId, { tags, generation: clock.generation });
      setRevision((value) => value + 1);
      return true;
    },
    [getClock]
  );

  const releaseActiveMutation = useCallback((token: AssetTagMutationToken) => {
    const key = `${token.epoch}:${token.assetId}:${token.generation}`;
    if (!activeMutationTokensRef.current.delete(key) || activeMutationTokensRef.current.size !== 0) return;
    const waiters = [...activeMutationDrainWaitersRef.current];
    activeMutationDrainWaitersRef.current.clear();
    for (const resolve of waiters) resolve();
  }, []);

  const beginMutation = useCallback((assetId: number): AssetTagMutationToken | null => {
    if (disposedRef.current || barrierRequestCountRef.current > 0) return null;
    const clock = getClock(assetId);
    if (clock.activeMutation !== null || clock.deleted) return null;
    clock.generation += 1;
    clock.activeMutation = clock.generation;
    const token = { assetId, epoch: epochRef.current, generation: clock.generation };
    activeMutationTokensRef.current.add(`${token.epoch}:${token.assetId}:${token.generation}`);
    return token;
  }, [getClock]);

  const settleMutation = useCallback((token: AssetTagMutationToken, rawTags?: string[]): boolean => {
    // Release barrier accounting even when reset/deletion made this token stale.
    releaseActiveMutation(token);
    if (token.epoch !== epochRef.current) return false;
    const clock = getClock(token.assetId);
    if (clock.activeMutation !== token.generation || clock.generation !== token.generation) return false;
    if (clock.deleted) {
      clock.generation += 1;
      clock.activeMutation = null;
      return false;
    }

    // Advancing again invalidates detail reads that started while the mutation was pending.
    clock.generation += 1;
    clock.activeMutation = null;
    const previous = entriesRef.current.get(token.assetId);
    if (rawTags !== undefined) {
      const tags = normalizeTags(rawTags);
      entriesRef.current.set(token.assetId, { tags, generation: clock.generation });
      setRevision((value) => value + 1);
    } else if (previous) {
      entriesRef.current.set(token.assetId, { ...previous, generation: clock.generation });
    }
    return true;
  }, [getClock, releaseActiveMutation]);

  const runWithMutationBarrier = useCallback(async <T,>(operation: () => Promise<T>): Promise<T> => {
    barrierRequestCountRef.current += 1;
    const previousBarrier = barrierTailRef.current;
    let releaseBarrier!: () => void;
    barrierTailRef.current = new Promise<void>((resolve) => {
      releaseBarrier = resolve;
    });

    try {
      await previousBarrier;
      if (disposedRef.current) throw new Error("Tag mutation coordinator is unavailable");
      if (activeMutationTokensRef.current.size > 0) {
        await new Promise<void>((resolve) => {
          activeMutationDrainWaitersRef.current.add(resolve);
        });
      }
      if (disposedRef.current) throw new Error("Tag mutation coordinator is unavailable");
      return await operation();
    } finally {
      barrierRequestCountRef.current -= 1;
      releaseBarrier();
    }
  }, []);

  const remove = useCallback((assetId: number, identityToken?: AssetTagGenerationToken): boolean => {
    if (identityToken && identityToken.epoch !== epochRef.current) return false;
    const clock = getClock(assetId);
    clock.generation += 1;
    clock.activeMutation = null;
    clock.deleted = true;
    entriesRef.current.delete(assetId);
    // Always notify consumers: deletion must invalidate local caches even when no tags were loaded.
    setRevision((value) => value + 1);
    return true;
  }, [getClock]);

  const reset = useCallback(() => {
    epochRef.current += 1;
    entriesRef.current.clear();
    clocksRef.current.clear();
    setEpoch(epochRef.current);
    setRevision((value) => value + 1);
  }, []);

  useEffect(() => {
    disposedRef.current = false;
    return () => {
      disposedRef.current = true;
      activeMutationTokensRef.current.clear();
      const waiters = [...activeMutationDrainWaitersRef.current];
      activeMutationDrainWaitersRef.current.clear();
      for (const resolve of waiters) resolve();
    };
  }, []);

  return useMemo(
    () => ({
      revision,
      epoch,
      get,
      captureGeneration,
      publishDetails,
      beginMutation,
      settleMutation,
      runWithMutationBarrier,
      remove,
      reset
    }),
    [
      beginMutation,
      captureGeneration,
      epoch,
      get,
      publishDetails,
      remove,
      reset,
      revision,
      runWithMutationBarrier,
      settleMutation
    ]
  );
}
