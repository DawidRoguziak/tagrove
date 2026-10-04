import { createSignal } from "solid-js";
import { applyTheme, readInitialTheme, type AppTheme } from "../ui/dom/theme";

export function createAppRuntime() {
  const [view, setView] = createSignal<"gallery" | "settings">("gallery");
  const [theme, setTheme] = createSignal(readInitialTheme());
  return {
    view,
    theme,
    openSettings: () => setView("settings"),
    closeSettings: () => setView("gallery"),
    setTheme(next: AppTheme) {
      applyTheme(next);
      setTheme(next);
    }
  };
}
