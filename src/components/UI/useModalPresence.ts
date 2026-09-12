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
    const entering = phase === "opening";
    const style = getComputedStyle(overlay);
    const token = style.getPropertyValue(entering ? "--modal-entrance" : "--modal-exit").trim()
      || style.getPropertyValue(entering ? "--motion-entrance" : "--motion-exit").trim();
    const milliseconds = Number.parseFloat(token) * (token.endsWith("ms") ? 1 : 1000);
    const duration = Number.isFinite(milliseconds) ? milliseconds : entering ? 150 : 100;
    const easing = style.getPropertyValue("--motion-ease").trim() || "cubic-bezier(0.2, 0, 0, 1)";
    // A fullscreen shell lives in the browser's top layer, outside ancestor opacity.
    const fullscreen = document.fullscreenElement;
    const targets = fullscreen instanceof HTMLElement && overlay.contains(fullscreen)
      ? [overlay, fullscreen] : [overlay];
    let cancelled = false;
    const finish = () => {
      if (!cancelled) setPhase(entering ? "open" : "hidden");
    };
    const animations = targets.map(target => {
      const from = target.style.opacity || (entering ? "0" : getComputedStyle(target).opacity || "1");
      target.style.opacity = entering ? "1" : "0";
      const animation = target.animate?.([{ opacity: from }, { opacity: target.style.opacity }], { duration, easing });
      if (animation) animation.onfinish = finish;
      return { target, animation };
    });
    // Covers unavailable animation events, including a suspended WebView.
    const timer = window.setTimeout(finish, duration);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      for (const { target, animation } of animations) {
        const opacity = getComputedStyle(target).opacity;
        if (animation) {
          animation.onfinish = null;
          animation.cancel();
        }
        target.style.opacity = opacity;
      }
    };
  }, [phase]);

  useLayoutEffect(() => {
    if (phase === "open") overlayRef.current?.style.removeProperty("opacity");
  }, [phase]);

  return { phase: nextPhase, present: nextPhase !== "hidden", overlayRef };
}
