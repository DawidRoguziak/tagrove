import { screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import "solid-js";
import "@solidjs/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Reset application modules to evaluate core under each fault. Keep the real Solid
// runtime shared with Testing Library, whose cleanup hook setup.ts already loaded.
vi.mock("solid-js", async (original) => original());
vi.mock("@solidjs/web", async (original) => original());
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: vi.fn(async (command: string) => {
    if (command === "get_startup_popular_tags" || command === "list_scan_roots") return [];
    if (command === "scan_startup_roots") return null;
    if (command === "get_video_tool_status")
      return { ffmpeg_available: true, ffprobe_available: true };
    if (command === "start_asset_query")
      return { status: "ready", session_id: 1, revision: 1, total: 0, offset: 0, items: [] };
    if (command === "list_tags") return { items: [], total: 0 };
  })
}));

const storage = window.localStorage;
let dispose: (() => void) | undefined;

beforeEach(() => {
  i18n.off("languageChanged");
  vi.resetModules();
  storage.clear();
  document.documentElement.removeAttribute("lang");
  document.documentElement.removeAttribute("data-theme");
  vi.spyOn(window.navigator, "language", "get").mockReturnValue("pl-PL");
});

afterEach(() => {
  dispose?.();
  dispose = undefined;
  i18n.off("languageChanged");
  vi.restoreAllMocks();
  Object.defineProperty(window, "localStorage", { configurable: true, value: storage });
  storage.clear();
  document.body.replaceChildren();
});

type Fault = "property" | "read" | "write" | "quota" | "none";
type Scope = "both" | "theme";

describe("preference storage during production bootstrap", () => {
  it.each<{
    fault: Fault;
    scope: Scope;
    browser: string;
    language: string;
    theme: string;
  }>([
    { fault: "property", scope: "both", browser: "pl-PL", language: "pl", theme: "dark" },
    { fault: "read", scope: "both", browser: "pl-PL", language: "pl", theme: "dark" },
    { fault: "read", scope: "both", browser: "unsupported", language: "en", theme: "dark" },
    { fault: "read", scope: "theme", browser: "pl-PL", language: "de", theme: "dark" },
    { fault: "write", scope: "both", browser: "pl-PL", language: "de", theme: "light" },
    { fault: "write", scope: "theme", browser: "pl-PL", language: "de", theme: "light" },
    { fault: "quota", scope: "both", browser: "pl-PL", language: "de", theme: "light" },
    { fault: "quota", scope: "theme", browser: "pl-PL", language: "de", theme: "light" },
    { fault: "none", scope: "both", browser: "pl-PL", language: "de", theme: "light" }
  ])("mounts and changes preferences with $fault failure, $scope, $browser", async (scenario) => {
    storage.setItem("media-tagger.language", "de-DE");
    storage.setItem("media-tagger.theme", "light");
    vi.spyOn(window.navigator, "language", "get").mockReturnValue(scenario.browser);
    const failure = new DOMException(
      "Synthetic preference storage failure",
      scenario.fault === "quota" ? "QuotaExceededError" : "SecurityError"
    );
    const affects = (key: string) => scenario.scope === "both" || key === "media-tagger.theme";
    if (scenario.fault === "property") {
      Object.defineProperty(window, "localStorage", {
        configurable: true,
        get() { throw failure; }
      });
    } else if (scenario.fault === "read") {
      const read = storage.getItem.bind(storage);
      vi.spyOn(storage, "getItem").mockImplementation((key) => {
        if (affects(key)) throw failure;
        return read(key);
      });
    } else if (scenario.fault === "write" || scenario.fault === "quota") {
      const write = storage.setItem.bind(storage);
      vi.spyOn(storage, "setItem").mockImplementation((key, value) => {
        if (affects(key)) throw failure;
        write(key, value);
      });
    }

    // No App/core/theme mock and no readiness override: module evaluation and
    // mounting must both survive the fault before any preference interaction.
    const { mountApplication } = await import("../mountApplication");
    const core = await import("../../i18n/core");
    const host = document.createElement("div");
    document.body.append(host);
    dispose = mountApplication(host);
    expect(await screen.findByTestId("gallery-grid")).toBeInTheDocument();
    expect(core.getActiveLanguage()).toBe(scenario.language);
    expect(document.documentElement).toHaveAttribute("lang", scenario.language);
    expect(document.documentElement).toHaveAttribute("data-theme", scenario.theme);
    if (scenario.fault === "none") {
      expect(storage.getItem(core.LANGUAGE_STORAGE_KEY)).toBe("de");
      expect(storage.getItem("media-tagger.theme")).toBe("light");
    }

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: core.default.t("topBar.openSettings") }));
    const language = await screen.findByRole("combobox");
    expect(language).toHaveValue(scenario.language);
    expect(screen.getByRole("radio", {
      name: core.default.t(`settings.appearance.theme.${scenario.theme}`)
    })).toBeChecked();

    const nextTheme = scenario.theme === "dark" ? "light" : "dark";
    await user.click(screen.getByRole("radio", {
      name: core.default.t(`settings.appearance.theme.${nextTheme}`)
    }));
    await user.selectOptions(language, "fr");
    await waitFor(() => expect(language).toHaveValue("fr"));
    expect(document.documentElement).toHaveAttribute("lang", "fr");
    expect(document.documentElement).toHaveAttribute("data-theme", nextTheme);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      core.default.t("settings.page.heading", { lng: "fr" })
    );
    expect(screen.getByRole("radio", {
      name: core.default.t(`settings.appearance.theme.${nextTheme}`)
    })).toBeChecked();
    if (scenario.fault === "none") {
      expect(storage.getItem(core.LANGUAGE_STORAGE_KEY)).toBe("fr");
      expect(storage.getItem("media-tagger.theme")).toBe(nextTheme);
    }
    await user.click(screen.getByRole("button", { name: core.default.t("common.back") }));
    await user.click(screen.getByRole("button", { name: core.default.t("topBar.openSettings") }));
    expect(await screen.findByRole("combobox")).toHaveValue("fr");
    expect(screen.getByRole("radio", {
      name: core.default.t(`settings.appearance.theme.${nextTheme}`)
    })).toBeChecked();
  });
});
