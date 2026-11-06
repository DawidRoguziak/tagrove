import { createSignal, onCleanup } from "solid-js";
import { normalizeTags } from "../../utils/media";

export interface AssetTagGenerationToken {
  assetId: number;
  epoch: number;
  generation: number;
}

export type AssetTagMutationToken = AssetTagGenerationToken;

/** One admitted metadata write. It reaches the head of every target ID's queue before `begin`. */
export interface AssetTagMutationTurn {
  /** Starts exclusive tokens synchronously; throws when the identity or asset is gone. */
  begin: () => AssetTagMutationToken[];
  /** Frees the queue slot and barrier accounting. Idempotent. */
  release: () => void;
}

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
  pin: (assetId: number) => () => void;
  isCurrent: (token: AssetTagGenerationToken) => boolean;
  waitForIdle: () => Promise<void>;
  readonly revision: number;
  readonly epoch: number;
  get: (assetId: number) => AuthoritativeAssetTags | null;
  captureGeneration: (assetId: number) => AssetTagGenerationToken;
  publishDetails: (assetId: number, tags: string[], token: AssetTagGenerationToken) => boolean;
  /**
   * Admits a write behind earlier writes on any of its IDs. Throws synchronously during
   * maintenance. Returns the turn directly when nothing is queued, so idle writes dispatch
   * in the same tick; otherwise resolves when the turn reaches the head of every queue.
   */
  enqueueMutation: (
    assetIds: readonly number[]
  ) => AssetTagMutationTurn | Promise<AssetTagMutationTurn>;
  settleMutation: (token: AssetTagMutationToken, tags?: string[]) => boolean;
  runWithMutationBarrier: <T>(operation: () => Promise<T>) => Promise<T>;
  remove: (assetId: number, identityToken?: AssetTagGenerationToken) => boolean;
  reset: () => void;
}

function sameTags(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index]);
}

let nextGeneration = 0;

/** Shell-owned source of complete, canonical tag lists shared by every tag editor. */
export function createAssetTagState(): AssetTagStateController {
  const entries = new Map<number, StoredAssetTags>();
  const clocks = new Map<number, AssetClock>();
  const pins = new Map<number, number>();
  const inactive = new Map<number, true>();
  let currentEpoch = 0;
  // Per-ID tail of the write queue. Admission links every target ID at once, so the
  // dependency graph follows admission order and overlapping bulk writes cannot deadlock.
  const tails = new Map<number, Promise<void>>();
  // Queued and running writes; maintenance and detail re-reads wait for both.
  let admitted = 0;
  const drainWaiters = new Set<() => void>();
  let barrierRequestCount = 0;
  let barrierTail: Promise<void> = Promise.resolve();
  let disposed = false;
  const [epoch, setEpoch] = createSignal(0);
  const [revision, setRevision] = createSignal(0);

  const prune = () => {
    while (inactive.size > 512) {
      const oldest = inactive.keys().next();
      if (oldest.done) break;
      const id = oldest.value;
      inactive.delete(id);
      clocks.delete(id);
      entries.delete(id);
    }
  };
  const pin = (id: number) => {
    inactive.delete(id);
    pins.set(id, (pins.get(id) ?? 0) + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      const count = pins.get(id) ?? 0;
      if (count <= 1) {
        pins.delete(id);
        const clock = clocks.get(id);
        if (clock && clock.activeMutation === null) inactive.set(id, true);
      } else pins.set(id, count - 1);
      prune();
    };
  };
  const isCurrent = (token: AssetTagGenerationToken) => {
    const clock = clocks.get(token.assetId);
    return (
      !disposed &&
      token.epoch === currentEpoch &&
      clock?.generation === token.generation &&
      clock.activeMutation === null &&
      !clock.deleted
    );
  };
  const waitForIdle = async () => {
    if (admitted > 0) await new Promise<void>((resolve) => drainWaiters.add(resolve));
  };
  const notifyDrained = () => {
    const waiters = [...drainWaiters];
    drainWaiters.clear();
    for (const resolve of waiters) resolve();
  };

  const getClock = (assetId: number): AssetClock => {
    let clock = clocks.get(assetId);
    if (!clock) {
      clock = { generation: ++nextGeneration, activeMutation: null, deleted: false };
      clocks.set(assetId, clock);
    }
    inactive.delete(assetId);
    if (!pins.has(assetId) && clock.activeMutation === null) inactive.set(assetId, true);
    prune();
    return clock;
  };

  const get = (assetId: number): AuthoritativeAssetTags | null => {
    revision();
    const entry = entries.get(assetId);
    if (entry && inactive.has(assetId)) {
      inactive.delete(assetId);
      inactive.set(assetId, true);
    }
    return entry ? { known: true, tags: entry.tags, generation: entry.generation } : null;
  };

  const captureGeneration = (assetId: number): AssetTagGenerationToken => {
    const clock = getClock(assetId);
    return { assetId, epoch: currentEpoch, generation: clock.generation };
  };

  const publishDetails = (
    assetId: number,
    rawTags: string[],
    token: AssetTagGenerationToken
  ): boolean => {
    const clock = getClock(assetId);
    if (
      disposed ||
      token.assetId !== assetId ||
      token.epoch !== currentEpoch ||
      token.generation !== clock.generation ||
      clock.activeMutation !== null ||
      clock.deleted
    ) {
      return false;
    }
    const tags = normalizeTags(rawTags);
    const previous = entries.get(assetId);
    if (previous && previous.generation === clock.generation && sameTags(previous.tags, tags)) {
      return true;
    }
    entries.set(assetId, { tags, generation: clock.generation });
    setRevision((value) => value + 1);
    return true;
  };

  const enqueueMutation = (
    assetIds: readonly number[]
  ): AssetTagMutationTurn | Promise<AssetTagMutationTurn> => {
    // Admission is synchronous: maintenance denies new writes before any queueing.
    if (disposed) throw new Error("Tag mutation coordinator is unavailable");
    if (barrierRequestCount > 0) throw new Error("Metadata changes are paused during maintenance.");
    const ids = [...new Set(assetIds)];
    const epoch = currentEpoch;
    let finish!: () => void;
    const finished = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const previous = new Set<Promise<void>>();
    for (const id of ids) {
      const tail = tails.get(id);
      if (tail) previous.add(tail);
      tails.set(id, finished);
    }
    admitted += 1;
    let used = false;
    let released = false;
    const turn: AssetTagMutationTurn = {
      begin() {
        if (used || released) throw new Error("Metadata write turn was already used");
        used = true;
        if (disposed) throw new Error("Tag mutation coordinator is unavailable");
        if (epoch !== currentEpoch)
          throw new Error("The library changed before this metadata change started.");
        if (ids.some((id) => clocks.get(id)?.deleted))
          throw new Error("The asset was deleted before this metadata change started.");
        if (ids.some((id) => (clocks.get(id)?.activeMutation ?? null) !== null))
          throw new Error("Metadata write started out of queue order");
        // One pass per ID: an active clock leaves the inactive LRU before the next prune.
        return ids.map((assetId) => {
          const clock = getClock(assetId);
          clock.generation = ++nextGeneration;
          clock.activeMutation = clock.generation;
          inactive.delete(assetId);
          return { assetId, epoch: currentEpoch, generation: clock.generation };
        });
      },
      release() {
        if (released) return;
        released = true;
        for (const id of ids) if (tails.get(id) === finished) tails.delete(id);
        finish();
        admitted -= 1;
        if (admitted === 0) notifyDrained();
      }
    };
    return previous.size ? Promise.all(previous).then(() => turn) : turn;
  };

  const settleMutation = (token: AssetTagMutationToken, rawTags?: string[]): boolean => {
    if (disposed || token.epoch !== currentEpoch) return false;
    const clock = getClock(token.assetId);
    if (clock.activeMutation !== token.generation || clock.generation !== token.generation)
      return false;
    if (clock.deleted) {
      clock.generation = ++nextGeneration;
      clock.activeMutation = null;
      return false;
    }

    // Advancing again invalidates detail reads that started while the mutation was pending.
    clock.generation = ++nextGeneration;
    clock.activeMutation = null;
    const previous = entries.get(token.assetId);
    if (rawTags !== undefined) {
      const tags = normalizeTags(rawTags);
      entries.set(token.assetId, { tags, generation: clock.generation });
      setRevision((value) => value + 1);
    } else if (previous) {
      entries.set(token.assetId, { ...previous, generation: clock.generation });
    }
    setRevision((value) => value + 1);
    if (!pins.has(token.assetId)) inactive.set(token.assetId, true);
    prune();
    return true;
  };

  const runWithMutationBarrier = async <T>(operation: () => Promise<T>): Promise<T> => {
    barrierRequestCount += 1;
    const previousBarrier = barrierTail;
    let releaseBarrier!: () => void;
    barrierTail = new Promise<void>((resolve) => {
      releaseBarrier = resolve;
    });

    try {
      await previousBarrier;
      if (disposed) throw new Error("Tag mutation coordinator is unavailable");
      // Writes admitted before the barrier, including queued ones, finish first.
      await waitForIdle();
      if (disposed) throw new Error("Tag mutation coordinator is unavailable");
      return await operation();
    } finally {
      barrierRequestCount -= 1;
      releaseBarrier();
    }
  };

  const remove = (assetId: number, identityToken?: AssetTagGenerationToken): boolean => {
    if (disposed || (identityToken && identityToken.epoch !== currentEpoch)) return false;
    const clock = getClock(assetId);
    clock.generation = ++nextGeneration;
    clock.activeMutation = null;
    clock.deleted = true;
    entries.delete(assetId);
    if (!pins.has(assetId)) inactive.set(assetId, true);
    prune();
    // Always notify consumers: deletion must invalidate local caches even when no tags were loaded.
    setRevision((value) => value + 1);
    return true;
  };

  const reset = () => {
    currentEpoch += 1;
    // A new identity starts with empty queues. Older admitted writes still count for
    // maintenance and are rejected by `begin` once they reach the head.
    tails.clear();
    entries.clear();
    clocks.clear();
    inactive.clear();
    setEpoch(currentEpoch);
    setRevision((value) => value + 1);
  };

  onCleanup(() => {
    disposed = true;
    notifyDrained();
  });

  return {
    pin,
    isCurrent,
    waitForIdle,
    get revision() {
      return revision();
    },
    get epoch() {
      return epoch();
    },
    get,
    captureGeneration,
    publishDetails,
    enqueueMutation,
    settleMutation,
    runWithMutationBarrier,
    remove,
    reset
  };
}
