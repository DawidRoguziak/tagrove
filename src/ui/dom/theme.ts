import { isTauri } from "../../platform/tauri/window";
import { syncNativeWindowTheme } from "../../platform/tauri/window";
export type AppTheme = "light" | "dark";

export const THEME_STORAGE_KEY = "media-tagger.theme";

export function readInitialTheme(): AppTheme {
  if (typeof window === "undefined") {
    return "dark";
  }

  const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (storedTheme === "light" || storedTheme === "dark") {
    return storedTheme;
  }

  return "dark";
}

export function applyTheme(theme: AppTheme) {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme);
  }

  if (typeof window !== "undefined") {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);

    if (isTauri()) {
      void syncNativeWindowTheme(theme).catch(() => {});
    }
  }
}
