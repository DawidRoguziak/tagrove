import { useCallback, useEffect, useRef, useState } from "react";

export function useLightboxActivity(active: boolean) {
  const [visible, setVisible] = useState(true);
  const [touch, setTouch] = useState(false);
  const visibleRef = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeRef = useRef(active);
  activeRef.current = active;

  const reveal = useCallback(() => {
    if (!activeRef.current) return;
    if (!visibleRef.current) {
      visibleRef.current = true;
      setVisible(true);
    }
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      visibleRef.current = false;
      setVisible(false);
    }, 3000);
  }, []);

  useEffect(() => {
    if (!active) return;
    const coarse = window.matchMedia?.("(hover: none) and (pointer: coarse)");
    const updateTouch = () => setTouch(coarse?.matches ?? false);
    updateTouch();
    coarse?.addEventListener("change", updateTouch);
    let lastPosition: { x: number; y: number } | null = null;
    const onMove = (event: MouseEvent) => {
      if (lastPosition?.x === event.clientX && lastPosition.y === event.clientY) return;
      lastPosition = { x: event.clientX, y: event.clientY };
      reveal();
    };
    const onPointerDown = (event: PointerEvent) => {
      if (event.pointerType === "touch") setTouch(true);
      reveal();
    };
    reveal();
    window.addEventListener("mousemove", onMove, { passive: true, capture: true });
    window.addEventListener("pointerdown", onPointerDown, { passive: true, capture: true });
    window.addEventListener("keydown", reveal, { capture: true });
    return () => {
      if (timer.current !== null) clearTimeout(timer.current);
      coarse?.removeEventListener("change", updateTouch);
      window.removeEventListener("mousemove", onMove, true);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", reveal, true);
    };
  }, [active, reveal]);

  return { visible: visible || touch, reveal };
}
