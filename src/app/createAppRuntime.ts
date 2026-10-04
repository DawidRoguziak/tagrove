import { createLibrary } from "../library/createLibrary";
import { createSignal } from "solid-js";
import { applyTheme, readInitialTheme, type AppTheme } from "../ui/dom/theme";

export function createAppRuntime() {
  const [view, setView] = createSignal<"gallery" | "settings">("gallery");
  const [theme, setTheme] = createSignal(readInitialTheme());
  const library = createLibrary();
  return {
    library,
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
