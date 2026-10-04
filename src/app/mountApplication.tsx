import { render } from "@solidjs/web";
import { i18nReady } from "../i18n/core";
import { applyTheme, readInitialTheme } from "../ui/dom/theme";
import App from "./App";

export function mountApplication(root: HTMLElement, ready: Promise<unknown> = i18nReady) {
  applyTheme(readInitialTheme());
  const preventContextMenu = (event: MouseEvent) => event.preventDefault();
  window.addEventListener("contextmenu", preventContextMenu, true);
  let observer: PerformanceObserver | undefined;
  if (import.meta.env.VITE_MEDIATAGGER_PERF === "1" && "PerformanceObserver" in window) {
    try {
      observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries())
          console.info(`[perf] name=${entry.name} duration_ms=${entry.duration.toFixed(1)}`);
      });
      observer.observe({ entryTypes: ["longtask", "measure"] });
    } catch {
      observer?.disconnect();
      observer = undefined;
    }
  }
  let disposed = false;
  let dispose: (() => void) | undefined;
  void ready.then(() => {
    if (!disposed) dispose = render(() => <App />, root);
  });
  return () => {
    disposed = true;
    dispose?.();
    observer?.disconnect();
    window.removeEventListener("contextmenu", preventContextMenu, true);
  };
}
