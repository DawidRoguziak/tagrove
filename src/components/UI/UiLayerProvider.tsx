import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject
} from "react";

interface UiLayerRegistration {
  id: symbol;
  modal: boolean;
  containerRef: RefObject<HTMLElement | null>;
  closeOnEscape: () => boolean;
  onEscape: () => void;
  restoreFocus: () => HTMLElement | null;
}

interface UiLayerManager {
  register: (layer: UiLayerRegistration) => () => void;
  topLayerId: symbol | null;
}

interface UseUiLayerOptions {
  active: boolean;
  modal: boolean;
  containerRef: RefObject<HTMLElement | null>;
  closeOnEscape?: boolean;
  onEscape: () => void;
  getRestoreFocus?: () => HTMLElement | null;
}

const UiLayerContext = createContext<UiLayerManager | null>(null);
let nextLayerId = 0;

const TABBABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])"
].join(",");

function getTabbableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(TABBABLE_SELECTOR)).filter(
    (element) => !element.hidden && !element.closest("[inert]")
  );
}

function focusInitialElement(container: HTMLElement) {
  const preferred = container.querySelector<HTMLElement>("[data-initial-focus]");
  const target = preferred ?? getTabbableElements(container)[0] ?? container;
  target.focus();
}

export function UiLayerProvider({ children }: { children: ReactNode }) {
  const layersRef = useRef<UiLayerRegistration[]>([]);
  const [topLayerId, setTopLayerId] = useState<symbol | null>(null);

  const managerRef = useRef<UiLayerManager | null>(null);
  if (managerRef.current === null) {
    managerRef.current = {
      topLayerId: null,
      register(layer) {
        layersRef.current = [...layersRef.current.filter((item) => item.id !== layer.id), layer];
        const nextTopLayerId = layer.id;
        setTopLayerId(nextTopLayerId);
        const frame = window.requestAnimationFrame(() => {
          const current = layersRef.current.at(-1);
          if (current?.id === layer.id && current.modal && current.containerRef.current) {
            focusInitialElement(current.containerRef.current);
          }
        });

        return () => {
          window.cancelAnimationFrame(frame);
          const wasTop = layersRef.current.at(-1)?.id === layer.id;
          layersRef.current = layersRef.current.filter((item) => item.id !== layer.id);
          const nextTop = layersRef.current.at(-1) ?? null;
          setTopLayerId(nextTop?.id ?? null);
          if (wasTop) {
            window.requestAnimationFrame(() => {
              const restoreTarget = layer.restoreFocus();
              if (restoreTarget?.isConnected) {
                restoreTarget.focus();
              } else if (nextTop?.containerRef.current) {
                focusInitialElement(nextTop.containerRef.current);
              }
            });
          }
        };
      }
    };
  }
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const topLayer = layersRef.current.at(-1);
      if (!topLayer) return;

      if (event.key === "Escape") {
        if (topLayer.closeOnEscape()) {
          event.preventDefault();
          event.stopImmediatePropagation();
          topLayer.onEscape();
        }
        return;
      }

      if (event.key !== "Tab" || !topLayer.modal || !topLayer.containerRef.current) return;
      const tabbable = getTabbableElements(topLayer.containerRef.current);
      if (tabbable.length === 0) {
        event.preventDefault();
        topLayer.containerRef.current.focus();
        return;
      }

      const first = tabbable[0];
      const last = tabbable[tabbable.length - 1];
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !topLayer.containerRef.current.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || !topLayer.containerRef.current.contains(active))) {
        event.preventDefault();
        first.focus();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useLayoutEffect(() => {
    const topLayer = layersRef.current.at(-1);
    const modalActive = Boolean(topLayer?.modal);
    const appRoot = document.getElementById("root");
    const previousOverflow = document.body.style.overflow;
    const layerElements = Array.from(document.querySelectorAll<HTMLElement>("[data-ui-layer]"));

    if (modalActive) {
      appRoot?.setAttribute("inert", "");
      appRoot?.setAttribute("aria-hidden", "true");
      document.body.style.overflow = "hidden";
      for (const element of layerElements) {
        if (element.dataset.uiLayer !== String(topLayerId?.description)) {
          element.setAttribute("inert", "");
          element.setAttribute("aria-hidden", "true");
        }
      }
    }

    return () => {
      appRoot?.removeAttribute("inert");
      appRoot?.removeAttribute("aria-hidden");
      document.body.style.overflow = previousOverflow;
      for (const element of layerElements) {
        element.removeAttribute("inert");
        element.removeAttribute("aria-hidden");
      }
    };
  }, [topLayerId]);

  return (
    <UiLayerContext.Provider value={{ register: managerRef.current.register, topLayerId }}>
      {children}
    </UiLayerContext.Provider>
  );
}

export function useUiLayer({
  active,
  modal,
  containerRef,
  closeOnEscape = true,
  onEscape,
  getRestoreFocus
}: UseUiLayerOptions) {
  const manager = useContext(UiLayerContext);
  const register = manager?.register;
  const idRef = useRef(Symbol(`ui-layer-${++nextLayerId}`));
  const onEscapeRef = useRef(onEscape);
  const closeOnEscapeRef = useRef(closeOnEscape);
  const getRestoreFocusRef = useRef(getRestoreFocus);
  onEscapeRef.current = onEscape;
  closeOnEscapeRef.current = closeOnEscape;
  getRestoreFocusRef.current = getRestoreFocus;

  useLayoutEffect(() => {
    if (!active || !register) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    return register({
      id: idRef.current,
      modal,
      containerRef,
      closeOnEscape: () => closeOnEscapeRef.current,
      onEscape: () => onEscapeRef.current(),
      restoreFocus: () => getRestoreFocusRef.current?.() ?? previouslyFocused
    });
  }, [active, containerRef, modal, register]);

  useEffect(() => {
    if (!active || register) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && closeOnEscapeRef.current) onEscapeRef.current();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, register]);

  return {
    isTopLayer: manager ? manager.topLayerId === idRef.current : true,
    layerId: idRef.current.description ?? "ui-layer"
  };
}
