import { createEffect, createSignal, onCleanup } from "solid-js";
import type { BulkSelectionInteraction, SelectionRange } from "../../components/gallery/selection";
import { containsIndex } from "../../components/gallery/services/selectionGeometry";

export function createSelection(options: {
  queryEpoch: () => number;
  identityEpoch: () => number;
  getIdsRange: (start: number, end: number) => Promise<number[]>;
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
      let rangeIndex = 0;
      let start = ranges[0]?.startIndex ?? 0;
      const last = ranges[ranges.length - 1]?.endIndex ?? -1;
      while (start <= last) {
        if (!current()) return;
        const end = Math.min(last, (Math.floor(start / 128) + 1) * 128 - 1);
        const page = await options.getIdsRange(start, end);
        if (!current()) return;
        if (page.length !== end - start + 1) throw new Error("Selection range is incomplete");
        page.forEach((id, offset) => {
          if (containsIndex(ranges, start + offset)) resolved.add(id);
        });
        while (rangeIndex < ranges.length && ranges[rangeIndex].endIndex <= end) rangeIndex++;
        start = Math.max(end + 1, ranges[rangeIndex]?.startIndex ?? last + 1);
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
