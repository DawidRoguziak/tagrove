import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";

export function createLightboxSidebar(active: () => boolean, locked: () => boolean) {
  const query = window.matchMedia?.("(max-width: 767px)");
  const [narrow, setNarrow] = createSignal(query?.matches ?? false);
  const changed = () => setNarrow(query?.matches ?? false);
  query?.addEventListener("change", changed);
  onCleanup(() => query?.removeEventListener("change", changed));
  const [choice, setChoice] = createSignal<boolean | null>(null);
  const open = createMemo(() => locked() || (choice() ?? !narrow()));
  const [retained, setRetained] = createSignal(untrack(open));
  createEffect(active, (active) => {
    if (!active) setChoice(null);
  });
  createEffect(open, (open) => {
    if (open) {
      setRetained(true);
      return;
    }
    const timer = setTimeout(
      () => setRetained(false),
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? 0 : 200
    );
    return () => clearTimeout(timer);
  });
  return {
    narrow,
    open,
    occupied: () => open() || retained(),
    show: () => setChoice(true),
    hide: () => {
      if (!locked()) setChoice(false);
    }
  };
}

export function createLightboxActivity(active: () => boolean) {
  const [visible, setVisible] = createSignal(true),
    [touch, setTouch] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const reveal = () => {
    if (!active()) return;
    setVisible(true);
    clearTimeout(timer);
    timer = setTimeout(() => setVisible(false), 3000);
  };
  createEffect(active, (active) => {
    if (!active) return;
    const query = window.matchMedia?.("(hover: none) and (pointer: coarse)");
    const changed = () => setTouch(query?.matches ?? false);
    changed();
    query?.addEventListener("change", changed);
    let position: { x: number; y: number } | null = null;
    const move = (event: PointerEvent) => {
      if (position?.x === event.clientX && position.y === event.clientY) return;
      position = { x: event.clientX, y: event.clientY };
      reveal();
    };
    const down = (event: PointerEvent) => {
      if (event.pointerType === "touch") setTouch(true);
      reveal();
    };
    window.addEventListener("pointermove", move, { passive: true, capture: true });
    window.addEventListener("pointerdown", down, { passive: true, capture: true });
    for (const name of ["wheel", "scroll", "keydown"])
      window.addEventListener(name, reveal, { passive: true, capture: true });
    untrack(reveal);
    return () => {
      clearTimeout(timer);
      query?.removeEventListener("change", changed);
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerdown", down, true);
      for (const name of ["wheel", "scroll", "keydown"])
        window.removeEventListener(name, reveal, true);
    };
  });
  return { visible: () => visible() || touch(), reveal };
}

export function createGroupClipboard(key: () => string, id: () => number) {
  const [confirmed, setConfirmed] = createSignal(false);
  let generation = 0,
    timer: ReturnType<typeof setTimeout> | undefined,
    disposed = false;
  createEffect(
    () => [key(), id()],
    () => {
      generation++;
      clearTimeout(timer);
      setConfirmed(false);
    }
  );
  onCleanup(() => {
    disposed = true;
    clearTimeout(timer);
  });
  const canCopy = () => !!key().trim() && typeof navigator.clipboard?.writeText === "function";
  return {
    confirmed,
    canCopy,
    async copy() {
      if (!canCopy()) return false;
      const request = generation,
        text = key().trim(),
        assetId = id();
      try {
        await navigator.clipboard.writeText(text);
        if (disposed || request !== generation || text !== key().trim() || assetId !== id())
          return false;
        setConfirmed(true);
        clearTimeout(timer);
        timer = setTimeout(() => setConfirmed(false), 1600);
        return true;
      } catch {
        if (!disposed && request === generation) setConfirmed(false);
        return false;
      }
    }
  };
}
