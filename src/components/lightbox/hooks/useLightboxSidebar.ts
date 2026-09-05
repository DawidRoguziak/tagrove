import { useCallback, useEffect, useState } from "react";

const NARROW_QUERY = "(max-width: 767px)";
export const SIDEBAR_TRANSITION_MS = 200;

export function useLightboxSidebar(active: boolean, locked: boolean) {
  const [isNarrow, setIsNarrow] = useState(
    () => window.matchMedia?.(NARROW_QUERY).matches ?? false
  );
  const [choice, setChoice] = useState<boolean | null>(null);
  const sidebarOpen = locked || (choice ?? !isNarrow);
  const [retainedOpen, setRetainedOpen] = useState(sidebarOpen);

  useEffect(() => {
    const query = window.matchMedia?.(NARROW_QUERY);
    if (!query) return;
    const update = () => setIsNarrow(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (!active) setChoice(null);
  }, [active]);

  useEffect(() => {
    if (sidebarOpen) {
      setRetainedOpen(true);
      return;
    }
    const delay = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
      ? 0
      : SIDEBAR_TRANSITION_MS;
    const timer = window.setTimeout(() => setRetainedOpen(false), delay);
    return () => window.clearTimeout(timer);
  }, [sidebarOpen]);

  const openSidebar = useCallback(() => setChoice(true), []);
  const closeSidebar = useCallback(() => {
    if (!locked) setChoice(false);
  }, [locked]);

  return {
    isNarrow,
    sidebarOpen,
    sidebarOccupied: sidebarOpen || retainedOpen,
    openSidebar,
    closeSidebar
  };
}
