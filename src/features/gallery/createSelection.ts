import { createEffect, createSignal, onCleanup } from "solid-js";
import type { BulkSelectionInteraction, SelectionRange } from "./selection";

export function createSelection(options: {
  queryEpoch: () => number;
  identityEpoch: () => number;
  /** Ordered IDs for an inclusive global range; rejects instead of truncating. */
  getIdsRange: (start: number, end: number) => Promise<ArrayLike<number>>;
}) {
  let enabled = false;
  let resolving = false;
  let ids = new Set<number>();
  let anchor: number | null = null;
  let generation = 0;
  let retry: (() => Promise<void>) | null = null;
  const [mode, setMode] = createSignal(false);
  const [selected, setSelected] = createSignal<ReadonlySet<number>>(ids);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const publish = (next: Set<number>) => {
    ids = next;
    setSelected(next);
  };
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
    publish(new Set());
  }
  createEffect(options.queryEpoch, () => {
    cancel();
    anchor = null;
  });
  createEffect(options.identityEpoch, clear);
  onCleanup(() => {
    generation++;
  });
  async function resolve(ranges: SelectionRange[], additive: boolean, clearAnchor: boolean) {
    cancel();
    const request = generation;
    const epoch = options.queryEpoch();
    const current = () => request === generation && epoch === options.queryEpoch();
    resolving = true;
    setBusy(true);
    try {
      const resolved = new Set<number>();
      // Ranges are ordered; one read covers their envelope (the query model
      // reads the backend session vector in a single binary IPC per 1M IDs).
      const first = ranges[0]?.startIndex ?? 0;
      const last = ranges[ranges.length - 1]?.endIndex ?? -1;
      if (first <= last) {
        const ids = await options.getIdsRange(first, last);
        if (!current()) return;
        if (ids.length !== last - first + 1) throw new Error("Selection range is incomplete");
        for (const range of ranges)
          for (let index = range.startIndex; index <= range.endIndex; index++)
            resolved.add(ids[index - first]);
      }
      if (!current()) return;
      publish(new Set(additive ? [...ids, ...resolved] : resolved));
      if (clearAnchor) anchor = null;
    } catch (cause) {
      if (current()) {
        setError(String(cause));
        retry = () => resolve(ranges, additive, clearAnchor);
      }
    } finally {
      if (current()) {
        resolving = false;
        setBusy(false);
      }
    }
  }
  function interact(interaction: BulkSelectionInteraction): void | Promise<void> {
    if (!enabled) return;
    cancel();
    switch (interaction.type) {
      case "clear":
        return clear();
      case "rectangle-start":
        resolving = true;
        setBusy(true);
        return;
      case "rectangle-cancel":
        return;
      case "rectangle-commit":
        return resolve(interaction.ranges, interaction.additive, true);
      case "click": {
        const { assetId, assetIndex, shift, ctrlLike } = interaction;
        if (shift && anchor !== null)
          return resolve(
            [{ startIndex: Math.min(anchor, assetIndex), endIndex: Math.max(anchor, assetIndex) }],
            ctrlLike,
            false
          );
        anchor = assetIndex;
        const next = new Set(ids);
        if (!shift && next.has(assetId)) next.delete(assetId);
        else next.add(assetId);
        publish(next);
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
    get ids() {
      return selected();
    },
    get busy() {
      return busy();
    },
    get error() {
      return error();
    },
    toggle() {
      cancel();
      enabled = !enabled;
      setMode(enabled);
      if (!enabled) clear();
    },
    capture: (): { ids: ReadonlySet<number>; enabled: boolean; busy: boolean } => ({
      ids,
      enabled,
      busy: resolving
    }),
    remove(idsToRemove: ReadonlySet<number>) {
      publish(new Set([...ids].filter((id) => !idsToRemove.has(id))));
    },
    clear,
    cancel,
    interact,
    retry: () => retry?.()
  };
}
