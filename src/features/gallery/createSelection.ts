import { createEffect, createSignal, onCleanup } from "solid-js";
import type { SelectionSpec } from "../../platform/tauri/contracts/assets";
import type { BulkSelectionInteraction, SelectionRange } from "./selection";

/**
 * Ranges covering at most this many positions are resolved to IDs right away, so small
 * selections keep their IDs across queries. Larger ones stay index ranges of their query
 * session and are resolved by the backend (stage 09, ADR 0008).
 */
export const MATERIALIZE_LIMIT = 10_000;

type Range = [number, number];
interface State {
  /** Inclusive, ascending, non-overlapping positions in `session`. */
  ranges: Range[];
  session: number | null;
  /** Unselected IDs inside `ranges`, with their position. */
  excluded: Map<number, number>;
  /** Selected IDs outside `ranges`, with their position in the current session or -1. */
  included: Map<number, number>;
}
const EMPTY: State = { ranges: [], session: null, excluded: new Map(), included: new Map() };

/** Sorts and merges overlapping or adjacent inclusive ranges. */
export function mergeRanges(ranges: readonly Range[]): Range[] {
  const sorted = ranges.filter(([start, end]) => start <= end).sort((a, b) => a[0] - b[0]);
  const merged: Range[] = [];
  for (const [start, end] of sorted) {
    const last = merged[merged.length - 1];
    if (last && start <= last[1] + 1) last[1] = Math.max(last[1], end);
    else merged.push([start, end]);
  }
  return merged;
}

/** Binary search: whether `index` lies in one of the sorted ranges. O(log r). */
export function rangesContain(ranges: readonly Range[], index: number): boolean {
  let low = 0;
  let high = ranges.length - 1;
  while (low <= high) {
    const middle = (low + high) >> 1;
    const [start, end] = ranges[middle];
    if (index < start) high = middle - 1;
    else if (index > end) low = middle + 1;
    else return true;
  }
  return false;
}

const coverage = (ranges: readonly Range[]) =>
  ranges.reduce((sum, [start, end]) => sum + end - start + 1, 0);

/**
 * Bulk selection as a spec instead of a set of every ID: ranges of the current query
 * session plus small `excluded`/`included` ID maps. Selecting a million results costs one
 * range; `isSelected` is a binary search and the count never materializes IDs.
 */
export function createSelection(options: {
  queryEpoch: () => number;
  identityEpoch: () => number;
  /** Ordered IDs for an inclusive global range; rejects instead of truncating. */
  getIdsRange: (start: number, end: number) => Promise<ArrayLike<number>>;
  /** Current backend query session, `null` while a query is pending. */
  sessionId?: () => number | null;
  /** Result count of the current query (select all). */
  total?: () => number;
}) {
  let enabled = false;
  let resolving = false;
  let state: State = EMPTY;
  let count = 0;
  let key = {};
  let idsCache: ReadonlySet<number> | null = null;
  let anchor: number | null = null;
  let generation = 0;
  let retry: (() => Promise<void>) | null = null;
  const [mode, setMode] = createSignal(false);
  const [version, setVersion] = createSignal(0);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal<"rangeCleared" | null>(null);
  const session = () => options.sessionId?.() ?? null;
  const publish = (next: State) => {
    state = next;
    count = coverage(next.ranges) - next.excluded.size + next.included.size;
    key = {};
    idsCache = null;
    setVersion((value) => value + 1);
  };
  const ids = (): ReadonlySet<number> => {
    if (!idsCache) idsCache = new Set(state.included.keys());
    return idsCache;
  };
  const spec = (): SelectionSpec | null =>
    state.ranges.length && state.session !== null
      ? {
          sessionId: state.session,
          ranges: state.ranges.map(([start, end]) => [start, end]),
          excluded: [...state.excluded.keys()],
          included: [...state.included.keys()]
        }
      : null;
  function cancel() {
    generation++;
    resolving = false;
    retry = null;
    setBusy(false);
    setError(null);
  }
  function clear() {
    cancel();
    anchor = null;
    setNotice(null);
    publish(EMPTY);
  }
  createEffect(options.queryEpoch, () => {
    cancel();
    anchor = null;
    const current = session();
    if (current === null) return;
    if (state.ranges.length && state.session !== current) {
      // Ranges are positions in the session the user saw; a new result (filter, scan,
      // reorder) gives them another meaning. Small ranges were already resolved to IDs.
      publish(EMPTY);
      setNotice("rangeCleared");
      return;
    }
    if (!state.ranges.length && state.session !== current && state.included.size) {
      // IDs stay selected; only their positions belonged to the old session. The selected
      // set, count and identity are unchanged, so nothing is published: a new `key` would
      // make the bulk panel treat this as a new selection and drop its drafts.
      state = {
        ...state,
        session: current,
        included: new Map([...state.included.keys()].map((id) => [id, -1]))
      };
    }
  });
  createEffect(options.identityEpoch, clear);
  onCleanup(() => {
    generation++;
  });
  /** Adds ranges (or replaces the selection with them), resolving IDs only when needed. */
  async function addRanges(added: Range[], additive: boolean, clearAnchor: boolean) {
    cancel();
    setNotice(null);
    const request = generation;
    const epoch = options.queryEpoch();
    const current = () => request === generation && epoch === options.queryEpoch();
    const base = additive ? state : EMPTY;
    const fresh = mergeRanges(added);
    const merged = mergeRanges([...base.ranges, ...fresh]);
    const unknown = [...base.included.values()].some((index) => index < 0);
    const finish = (next: State) => {
      publish(next);
      if (clearAnchor) anchor = null;
    };
    try {
      const target = session();
      if (target === null || (base.ranges.length && base.session !== target))
        throw new Error("Selection range was cancelled");
      if (!merged.length) return finish(base);
      const materialize = coverage(merged) <= MATERIALIZE_LIMIT;
      if (!materialize && !unknown) {
        // A large range is selected without reading a single ID.
        const excluded = new Map(
          [...base.excluded].filter(([, index]) => !rangesContain(fresh, index))
        );
        const included = new Map(
          [...base.included].filter(([, index]) => !rangesContain(merged, index))
        );
        return finish({ ranges: merged, session: target, excluded, included });
      }
      // Read IDs in one binary IPC per 1M: all ranges when small, else only the new ones
      // (to find included IDs whose position in this session is unknown).
      const scope = materialize ? merged : fresh;
      const first = scope[0][0];
      const last = scope[scope.length - 1][1];
      resolving = true;
      setBusy(true);
      const read = await options.getIdsRange(first, last);
      if (!current()) return;
      if (read.length !== last - first + 1) throw new Error("Selection range is incomplete");
      const included = new Map(base.included);
      if (materialize) {
        for (const [start, end] of merged)
          for (let index = start; index <= end; index++) {
            const id = read[index - first];
            if (!base.excluded.has(id)) included.set(id, index);
          }
        return finish({ ranges: [], session: target, excluded: new Map(), included });
      }
      for (const [start, end] of fresh)
        for (let index = start; index <= end; index++) {
          const id = read[index - first];
          if (included.get(id) === -1) included.set(id, index);
        }
      return finish({
        ranges: merged,
        session: target,
        excluded: new Map([...base.excluded].filter(([, index]) => !rangesContain(fresh, index))),
        included: new Map([...included].filter(([, index]) => !rangesContain(merged, index)))
      });
    } catch (cause) {
      if (current()) {
        setError(String(cause));
        retry = () => addRanges(added, additive, clearAnchor);
      }
    } finally {
      if (current()) {
        resolving = false;
        setBusy(false);
      }
    }
  }
  const toRanges = (ranges: SelectionRange[]): Range[] =>
    ranges.map((range) => [range.startIndex, range.endIndex]);
  function interact(interaction: BulkSelectionInteraction): void | Promise<void> {
    if (!enabled) return;
    cancel();
    switch (interaction.type) {
      case "clear":
        return clear();
      case "select-all": {
        const total = options.total?.() ?? 0;
        if (total <= 0) return;
        return addRanges([[0, total - 1]], false, true);
      }
      case "rectangle-start":
        resolving = true;
        setBusy(true);
        return;
      case "rectangle-cancel":
        return;
      case "rectangle-commit":
        return addRanges(toRanges(interaction.ranges), interaction.additive, true);
      case "click": {
        const { assetId, assetIndex, shift, ctrlLike } = interaction;
        if (shift && anchor !== null)
          return addRanges(
            [[Math.min(anchor, assetIndex), Math.max(anchor, assetIndex)]],
            ctrlLike,
            false
          );
        anchor = assetIndex;
        setNotice(null);
        const index = session() === null ? -1 : assetIndex;
        if (state.ranges.length && rangesContain(state.ranges, assetIndex)) {
          const excluded = new Map(state.excluded);
          if (!shift && !excluded.has(assetId)) excluded.set(assetId, assetIndex);
          else excluded.delete(assetId);
          publish({ ...state, excluded });
          return;
        }
        const included = new Map(state.included);
        if (!shift && included.has(assetId)) included.delete(assetId);
        else included.set(assetId, index);
        publish({ ...state, session: state.session ?? session(), included });
        return;
      }
      default: {
        const exhaustive: never = interaction;
        return exhaustive;
      }
    }
  }
  return {
    get enabled() {
      return mode();
    },
    /** Selected IDs held directly; with ranges these are only the IDs outside them. */
    get ids() {
      version();
      return ids();
    },
    /** Whether the selection holds index ranges that only the backend resolves. */
    get ranged() {
      version();
      return state.ranges.length > 0;
    },
    get count() {
      version();
      return count;
    },
    /** Changes identity on every selection change. */
    get key() {
      version();
      return key;
    },
    get revision() {
      return version();
    },
    get busy() {
      return busy();
    },
    get error() {
      return error();
    },
    /** Why the selection was cleared without a user action, if it was. */
    get notice() {
      return notice();
    },
    /** O(log r). Without an `id` (a placeholder tile) only the ranges are consulted. */
    isSelected(index: number, id?: number) {
      version();
      if (state.ranges.length && rangesContain(state.ranges, index))
        return id === undefined || !state.excluded.has(id);
      return id !== undefined && state.included.has(id);
    },
    toggle() {
      cancel();
      enabled = !enabled;
      setMode(enabled);
      if (!enabled) clear();
    },
    capture: () => ({
      ids: ids(),
      spec: spec(),
      count,
      key,
      enabled,
      busy: resolving
    }),
    remove(idsToRemove: ReadonlySet<number>) {
      const included = new Map([...state.included].filter(([id]) => !idsToRemove.has(id)));
      publish({ ...state, included });
    },
    clear,
    cancel,
    interact,
    retry: () => retry?.()
  };
}
