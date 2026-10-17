import { isTauri } from "../../platform/tauri/window";
import { syncNativeWindowTheme } from "../../platform/tauri/window";
export type AppTheme = "light" | "dark";

export const THEME_STORAGE_KEY = "media-tagger.theme";

export function readInitialTheme(): AppTheme {
  if (typeof window === "undefined") {
    return "dark";
  }

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (storedTheme === "light" || storedTheme === "dark") {
      return storedTheme;
    }
  } catch {
    // Use the default when preference storage is unavailable.
  }

  return "dark";
}

export function applyTheme(theme: AppTheme) {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme);
  }

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // DOM and native theme changes do not require persistence.
    }

    if (isTauri()) {
      void syncNativeWindowTheme(theme).catch(() => {});
    }
  }
}
