import { createContext, createEffect, onCleanup, useContext } from "solid-js";
import type { JSX } from "@solidjs/web";

interface Layer {
  id: string;
  modal: boolean;
  container: () => HTMLElement | undefined;
  closeOnEscape: () => boolean;
  onEscape: () => void;
  restoreFocus: () => HTMLElement | null;
}
const selector =
  'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
function tabbable(container: HTMLElement) {
  return Array.from(container.querySelectorAll<HTMLElement>(selector)).filter(
    (el) => !el.hidden && !el.closest("[inert]")
  );
}
function focusInitial(container: HTMLElement) {
  (
    container.querySelector<HTMLElement>("[data-initial-focus]") ??
    tabbable(container)[0] ??
    container
  ).focus();
}
function createLayerManager() {
  let layers: Layer[] = [];
  let undoLocks = () => {};
  let disposed = false;
  const frames = new Set<number>();
  function frame(callback: () => void) {
    const id = requestAnimationFrame(() => {
      frames.delete(id);
      if (!disposed) callback();
    });
    frames.add(id);
    return id;
  }
  function syncLocks() {
    undoLocks();
    const top = layers[layers.length - 1];
    if (!top?.modal) {
      undoLocks = () => {};
      return;
    }
    const targets = [
      document.getElementById("root"),
      ...document.querySelectorAll<HTMLElement>("[data-ui-layer]")
    ].filter(
      (el): el is HTMLElement =>
        !!el && el.dataset.uiLayer !== top.id && el.dataset.modalPresence !== "closing"
    );
    const saved = targets.map((el) => ({
      el,
      inert: el.getAttribute("inert"),
      hidden: el.getAttribute("aria-hidden")
    }));
    const overflow = document.body.style.overflow;
    for (const el of targets) {
      el.setAttribute("inert", "");
      el.setAttribute("aria-hidden", "true");
    }
    document.body.style.overflow = "hidden";
    undoLocks = () => {
      document.body.style.overflow = overflow;
      for (const { el, inert, hidden } of saved) {
        if (inert === null) el.removeAttribute("inert");
        else el.setAttribute("inert", inert);
        if (el.dataset.modalPresence === "closing") continue;
        if (hidden === null) el.removeAttribute("aria-hidden");
        else el.setAttribute("aria-hidden", hidden);
      }
    };
  }
  function keydown(event: KeyboardEvent) {
    const top = layers[layers.length - 1];
    if (!top) return;
    if (event.key === "Escape") {
      if (top.closeOnEscape()) {
        event.preventDefault();
        event.stopImmediatePropagation();
        top.onEscape();
      }
      return;
    }
    const container = top.container();
    if (event.key !== "Tab" || !top.modal || !container) return;
    const elements = tabbable(container);
    const first = elements[0],
      last = elements[elements.length - 1];
    if (!first || !last) {
      event.preventDefault();
      container.focus();
      return;
    }
    const active = document.activeElement;
    if (event.shiftKey && (active === first || !container.contains(active))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (active === last || !container.contains(active))) {
      event.preventDefault();
      first.focus();
    }
  }
  window.addEventListener("keydown", keydown);
  onCleanup(() => {
    disposed = true;
    window.removeEventListener("keydown", keydown);
    for (const id of frames) cancelAnimationFrame(id);
    undoLocks();
    layers = [];
  });
  return {
    isTop: (id: string) => layers[layers.length - 1]?.id === id,
    register(layer: Layer) {
      layers.push(layer);
      syncLocks();
      const initial = frame(() => {
        if (layers[layers.length - 1] === layer && layer.modal) {
          const el = layer.container();
          if (el) focusInitial(el);
        }
      });
      return () => {
        cancelAnimationFrame(initial);
        frames.delete(initial);
        const index = layers.indexOf(layer);
        const wasTop = index === layers.length - 1;
        // A modal above the removed layer owns focus; nonmodal layers above it do not.
        const coveredByModal = layers.slice(index + 1).some((item) => item.modal);
        const remaining = new Set(layers);
        remaining.delete(layer);
        layers = layers.filter((item) => item !== layer);
        if (disposed || index < 0) return;
        syncLocks();
        const next = layers[layers.length - 1];
        if (coveredByModal) return;
        frame(() => {
          // Only a modal registered after this release takes focus over the restore.
          if (layers.some((item) => item.modal && !remaining.has(item))) return;
          // Below a nonmodal layer, restore only focus that was lost with this layer.
          if (!wasTop) {
            const active = document.activeElement;
            if (active && active !== document.body && active.isConnected) return;
          }
          const restore = layer.restoreFocus();
          if (restore?.isConnected && !restore.closest("[inert]")) restore.focus();
          else if (next && layers.includes(next)) {
            const el = next.container();
            if (el) focusInitial(el);
          }
        });
      };
    }
  };
}
const LayerContext = createContext<ReturnType<typeof createLayerManager> | null>(null);
export function UiLayerProvider(props: { children: JSX.Element }) {
  const manager = createLayerManager();
  return <LayerContext value={manager}>{props.children}</LayerContext>;
}
let nextLayer = 0;
export function createUiLayer(options: {
  active: () => boolean;
  modal: boolean;
  container: () => HTMLElement | undefined;
  closeOnEscape?: () => boolean;
  onEscape: () => void;
  getRestoreFocus?: () => HTMLElement | null;
}) {
  const manager = useContext(LayerContext);
  const id = `solid-ui-layer-${++nextLayer}`;
  createEffect(options.active, (active) => {
    if (!active) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (manager)
      return manager.register({
        id,
        modal: options.modal,
        container: options.container,
        closeOnEscape: () => options.closeOnEscape?.() ?? true,
        onEscape: options.onEscape,
        restoreFocus: () => options.getRestoreFocus?.() ?? previous
      });
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && (options.closeOnEscape?.() ?? true)) options.onEscape();
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  });
  return { layerId: id, isTopLayer: () => manager?.isTop(id) ?? true };
}
