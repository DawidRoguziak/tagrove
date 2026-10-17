import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { syncNativeWindowTheme } from "../../../platform/tauri/window";
import { applyTheme, readInitialTheme, THEME_STORAGE_KEY } from "../theme";

vi.mock("../../../platform/tauri/window", () => ({
  isTauri: () => true,
  syncNativeWindowTheme: vi.fn().mockResolvedValue(undefined)
}));

describe("themeService", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
  });

  it("uses dark theme by default", () => {
    expect(readInitialTheme()).toBe("dark");
  });

  it("reads stored light or dark theme", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "light");
    expect(readInitialTheme()).toBe("light");

    window.localStorage.setItem(THEME_STORAGE_KEY, "dark");
    expect(readInitialTheme()).toBe("dark");
  });

  it("ignores invalid stored theme", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "solarized");
    expect(readInitialTheme()).toBe("dark");
  });

  it("applies theme to document and local storage", () => {
    applyTheme("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");

    applyTheme("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
  });

  it("still synchronizes the native theme when persistence fails", () => {
    vi.spyOn(window.localStorage, "setItem").mockImplementation(() => {
      throw new DOMException("Synthetic quota failure", "QuotaExceededError");
    });

    applyTheme("light");

    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(syncNativeWindowTheme).toHaveBeenCalledExactlyOnceWith("light");
  });
});
