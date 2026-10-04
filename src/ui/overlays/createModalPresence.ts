import { animateModal } from "../dom/modalMotion";
import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";

type Phase = "opening" | "open" | "closing" | "hidden";
export function createModalSnapshot<T>(open: () => boolean, value: () => T) {
  return createMemo<T>((previous) => (open() || previous === undefined ? value() : previous));
}
export function createModalPresence(open: () => boolean) {
  const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
  const [reduced, setReduced] = createSignal(query?.matches ?? false);
  const changed = () => setReduced(query?.matches ?? false);
  query?.addEventListener("change", changed);
  onCleanup(() => query?.removeEventListener("change", changed));
  const [finished, setFinished] = createSignal<{ open: boolean; generation: number } | null>(null);
  const transition = createMemo<{ open: boolean; generation: number; wasPresent: boolean }>(
    (previous) => {
      const active = open();
      if (previous?.open === active) return previous;
      return {
        open: active,
        generation: (previous?.generation ?? 0) + 1,
        wasPresent: previous !== undefined
      };
    }
  );
  const phase = createMemo<Phase>(() => {
    const current = transition();
    if (reduced() || finished()?.generation === current.generation)
      return current.open ? "open" : "hidden";
    return current.open ? "opening" : current.wasPresent ? "closing" : "hidden";
  });
  const [overlay, setOverlay] = createSignal<HTMLDivElement>();
  createEffect(
    () => ({ phase: phase(), overlay: overlay(), transition: transition() }),
    (state) => {
      const el = state.overlay;
      if (!el) return;
      if (state.phase === "open") {
        el.style.removeProperty("opacity");
        return;
      }
      if (state.phase !== "opening" && state.phase !== "closing") return;
      return animateModal(el, state.phase === "opening", () => setFinished(state.transition));
    }
  );
  return {
    phase,
    present: () => phase() !== "hidden",
    overlayRef: (el: HTMLDivElement) => untrack(() => setOverlay(el))
  };
}
