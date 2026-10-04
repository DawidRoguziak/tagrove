import { animateModal } from "../../ui/dom/modalMotion";
import { useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";

const reducedMotionQuery = "(prefers-reduced-motion: reduce)";
function subscribeReducedMotion(notify: () => void) {
  const query = window.matchMedia?.(reducedMotionQuery);
  query?.addEventListener("change", notify);
  return () => query?.removeEventListener("change", notify);
}
function getReducedMotion() {
  return window.matchMedia?.(reducedMotionQuery).matches ?? false;
}

type ModalPhase = "opening" | "open" | "closing" | "hidden";

// Retain the last committed visible props/children when their owner clears state.
export function useModalSnapshot<T>(open: boolean, present: boolean, value: T): T {
  const lastVisible = useRef(value);
  useLayoutEffect(() => {
    if (open || !present) lastVisible.current = value;
  });
  return open ? value : lastVisible.current;
}

export function useModalPresence(open: boolean) {
  const reducedMotion = useSyncExternalStore(subscribeReducedMotion, getReducedMotion);
  const overlayRef = useRef<HTMLDivElement>(null);
  const [phase, setPhase] = useState<ModalPhase>(open ? "opening" : "hidden");
  let nextPhase = phase;
  if (reducedMotion) nextPhase = open ? "open" : "hidden";
  else if (open && (phase === "hidden" || phase === "closing")) nextPhase = "opening";
  else if (!open && (phase === "opening" || phase === "open")) nextPhase = "closing";
  if (phase !== nextPhase) setPhase(nextPhase);

  useLayoutEffect(() => {
    const overlay = overlayRef.current;
    if (!overlay || (phase !== "opening" && phase !== "closing")) return;
    return animateModal(overlay, phase === "opening", () => setPhase(phase === "opening" ? "open" : "hidden"));
  }, [phase]);

  useLayoutEffect(() => {
    if (phase === "open") overlayRef.current?.style.removeProperty("opacity");
  }, [phase]);

  return { phase: nextPhase, present: nextPhase !== "hidden", overlayRef };
}
