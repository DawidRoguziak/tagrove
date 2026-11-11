import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal, flush, lazy, untrack } from "solid-js";
import type { JSX } from "@solidjs/web";
import App from "../App";
import { APP_LANGUAGES } from "../../i18n/languages";
import i18n, { changeAppLanguage, LANGUAGE_STORAGE_KEY } from "../../i18n/core";
import { LanguageProvider, useLanguage } from "../../i18n/solid";
import { applyTheme, readInitialTheme, THEME_STORAGE_KEY } from "../../ui/dom/theme";
import { mountApplication } from "../mountApplication";
import { LazyErrorBoundary } from "../../ui/overlays/LazyErrorBoundary";
import LightboxBoundary from "../LightboxBoundary";
import { UiLayerProvider } from "../../ui/overlays/UiLayerProvider";
const native = vi.hoisted(() => ({
  available: false,
  invoke: vi.fn().mockResolvedValue(undefined)
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => native.available, invoke: native.invoke }));
afterEach(async () => {
  cleanup();
  await changeAppLanguage("en");
  localStorage.clear();
  native.available = false;
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe("Solid shell", () => {
  beforeEach(() => {
    native.invoke.mockImplementation(async (command: string) => {
      if (command === "list_scan_roots") return [];
      if (command === "scan_startup_roots") return null;
      if (command === "get_video_tool_status")
        return { ffmpeg_available: true, ffprobe_available: true };
      if (command === "start_asset_query")
        return { status: "ready", session_id: 1, revision: 1, total: 0, offset: 0, items: [] };
      if (command === "list_tags") return { items: [], total: 0 };
      if (command === "get_startup_popular_tags") return [];
      if (command === "get_asset_query_sections")
        return { status: "ready", session_id: 1, total: 0, sections: [] };
    });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    );
  });
  it("highlights scanning only for the first-folder route and clears it on Back", async () => {
    render(() => <App />);
    await userEvent.click(await screen.findByRole("button", { name: "Add first folder" }));
    expect(await screen.findByTestId("scan-settings-section")).toHaveAttribute(
      "data-highlighted",
      "true"
    );
    await userEvent.click(screen.getByRole("button", { name: "Back" }));
    await userEvent.click(screen.getByRole("button", { name: "Open settings" }));
    expect(await screen.findByTestId("scan-settings-section")).toHaveAttribute(
      "data-highlighted",
      "false"
    );
  });
  it("switches views, translates live, persists appearance and returns with Escape", async () => {
    render(() => <App />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Open settings" }));
    const language = await screen.findByRole("combobox");
    expect([...language.querySelectorAll("option")].map(option => ({ value: option.value, label: option.textContent }))).toEqual([
      { value: "en", label: "English" },
      { value: "pl", label: "Polski" },
      { value: "fr", label: "Français" },
      { value: "de", label: "Deutsch" },
      { value: "it", label: "Italiano" },
      { value: "es", label: "Español" },
      { value: "ru", label: "Русский" },
      { value: "zh", label: "中文（简体）" },
      { value: "ja", label: "日本語" },
      { value: "ko", label: "한국어" },
      { value: "cs", label: "Čeština" }
    ]);
    await user.click(screen.getByRole("radio", { name: "Light" }));
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
    await user.selectOptions(language, "pl");
    await waitFor(() => expect(document.documentElement.lang).toBe("pl"));
    expect(localStorage.getItem(LANGUAGE_STORAGE_KEY)).toBe("pl");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      i18n.t("settings.page.heading")
    );
    fireEvent.keyDown(window, { key: "Escape" });
    flush();
    expect(screen.getByTestId("gallery-grid")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: i18n.t("topBar.openSettings") }));
    expect(await screen.findByRole("combobox")).toHaveValue("pl");
    expect(
      screen.getByRole("radio", { name: i18n.t("settings.appearance.theme.light") })
    ).toBeChecked();
  });
  it.each([false, true])(
    "restores focus to the settings opener after Escape (bulk mode %s)",
    async (bulk) => {
      render(() => <App />);
      const user = userEvent.setup();
      if (bulk) {
        await user.click(await screen.findByRole("button", { name: "Enable bulk actions" }));
        await screen.findByRole("button", { name: "Disable bulk actions" });
      }
      await user.click(screen.getByRole("button", { name: "Open settings" }));
      (await screen.findByRole("button", { name: "Back" })).focus();
      fireEvent.keyDown(window, { key: "Escape" });
      flush();
      expect(screen.getByTestId("gallery-grid")).toBeInTheDocument();
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Open settings" })).toHaveFocus()
      );
      if (bulk)
        expect(screen.getByRole("button", { name: "Disable bulk actions" })).toHaveAttribute(
          "aria-pressed",
          "true"
        );
    }
  );
  it("owns one language subscription per app and removes it across remounts", async () => {
    const on = vi.spyOn(i18n, "on"),
      off = vi.spyOn(i18n, "off");
    function Text() {
      const { t } = useLanguage();
      return <span>{t("common.back")}</span>;
    }
    for (let i = 0; i < 2; i++) {
      const view = render(() => (
        <LanguageProvider>
          <Text />
          <Text />
        </LanguageProvider>
      ));
      expect(on.mock.calls.filter(([event]) => event === "languageChanged")).toHaveLength(i + 1);
      await changeAppLanguage(i === 0 ? "pl" : "en");
      flush();
      expect(screen.getAllByText(i18n.t("common.back"))).toHaveLength(2);
      view.unmount();
      const listener = on.mock.calls.filter(([event]) => event === "languageChanged")[i][1];
      expect(off).toHaveBeenCalledWith("languageChanged", listener);
    }
  });
  it("keeps all eleven locales reactive with normalized document/storage values", async () => {
    function Label() {
      const language = useLanguage();
      return <p>{language.t("common.back")}</p>;
    }
    render(() => (
      <LanguageProvider>
        <Label />
      </LanguageProvider>
    ));
    for (const language of APP_LANGUAGES) {
      await changeAppLanguage(language);
      flush();
      expect(screen.getByText(i18n.t("common.back"))).toBeInTheDocument();
      expect(document.documentElement.lang).toBe(language);
      expect(localStorage.getItem(LANGUAGE_STORAGE_KEY)).toBe(language);
    }
    await i18n.changeLanguage("pl-PL");
    flush();
    expect(document.documentElement.lang).toBe("pl");
    await i18n.changeLanguage("unsupported");
    flush();
    expect(document.documentElement.lang).toBe("en");
    expect(screen.getByText("Back")).toBeInTheDocument();
  });

  it("preserves theme fallback and invokes native synchronization only under Tauri", async () => {
    localStorage.setItem(THEME_STORAGE_KEY, "unknown");
    expect(readInitialTheme()).toBe("dark");
    applyTheme("light");
    expect(native.invoke).not.toHaveBeenCalled();
    native.available = true;
    applyTheme("dark");
    expect(native.invoke).toHaveBeenCalledWith("sync_window_theme", { theme: "dark" });
    native.invoke.mockRejectedValueOnce(new Error("native unavailable"));
    applyTheme("light");
    await Promise.resolve();
    expect(readInitialTheme()).toBe("light");
  });
  it("applies theme before readiness, suppresses context menus and cancels late mount", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    let ready = () => {};
    const pending = new Promise<void>((resolve) => {
      ready = resolve;
    });
    localStorage.setItem(THEME_STORAGE_KEY, "light");
    const stop = mountApplication(host, pending);
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(host).toBeEmptyDOMElement();
    const event = new MouseEvent("contextmenu", { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    stop();
    ready();
    await Promise.resolve();
    expect(host).toBeEmptyDOMElement();
    const next = new MouseEvent("contextmenu", { cancelable: true });
    window.dispatchEvent(next);
    expect(next.defaultPrevented).toBe(false);
    const stopRemount = mountApplication(host, Promise.resolve());
    await Promise.resolve();
    expect(host).toHaveTextContent("Tagrove");
    stopRemount();
    expect(host).toBeEmptyDOMElement();
    host.remove();
  });
  it("disposes optional performance instrumentation with the mounted application", () => {
    const observe = vi.fn(),
      disconnect = vi.fn();
    vi.stubEnv("VITE_MEDIATAGGER_PERF", "1");
    vi.stubGlobal(
      "PerformanceObserver",
      class {
        observe = observe;
        disconnect = disconnect;
      }
    );
    try {
      const host = document.createElement("div");
      const stop = mountApplication(host, new Promise(() => {}));
      expect(observe).toHaveBeenCalledWith({ entryTypes: ["longtask", "measure"] });
      stop();
      expect(disconnect).toHaveBeenCalledOnce();
    } finally {
      vi.unstubAllGlobals();
      vi.unstubAllEnvs();
    }
  });

  it("cancels the 1500 ms prefetch timer on disposal", () => {
    vi.useFakeTimers();
    try {
      const view = render(() => <App />);
      expect(vi.getTimerCount()).toBe(1);
      view.unmount();
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });
  it("shows lazy errors, keeps them until the reset key changes, and then recovers", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const [fail, setFail] = createSignal(true),
      [key, setKey] = createSignal(0);
    function Fragile() {
      if (untrack(fail)) throw new Error("chunk failed");
      return <span>Recovered</span>;
    }
    render(() => (
      <LazyErrorBoundary resetKey={key()} fallback={<p role="alert">Failed</p>}>
        <Fragile />
      </LazyErrorBoundary>
    ));
    expect(screen.getByRole("alert")).toHaveTextContent("Failed");
    setFail(false);
    flush();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    setKey(1);
    flush();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Recovered")).toBeInTheDocument();
  });
  it("keeps a healthy lazy view mounted when the reset key changes", () => {
    const [key, setKey] = createSignal(1);
    const mounts = vi.fn();
    function Media() {
      mounts();
      return <span>Healthy media</span>;
    }
    render(() => <LazyErrorBoundary resetKey={key()} fallback={<p>Failed</p>}><Media /></LazyErrorBoundary>);
    const media = screen.getByText("Healthy media");
    setKey(2);
    flush();
    expect(screen.getByText("Healthy media")).toBe(media);
    expect(mounts).toHaveBeenCalledTimes(1);
  });
  it("uses a named dialog while a lazy lightbox loads, then releases it", async () => {
    let complete: (module: { default: () => JSX.Element }) => void = () => {};
    const pending = new Promise<{ default: () => JSX.Element }>((resolve) => {
      complete = resolve;
    });
    const Media = lazy(() => pending);
    render(() => (
      <LanguageProvider>
        <UiLayerProvider>
          <LightboxBoundary open name="synthetic.png" resetKey={1} onClose={() => {}}>
            <Media />
          </LightboxBoundary>
        </UiLayerProvider>
      </LanguageProvider>
    ));
    expect(screen.getByRole("dialog")).toHaveAccessibleName(
      i18n.t("lightbox.previewDialog", { name: "synthetic.png" })
    );
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    complete({ default: () => <p>Loaded media</p> });
    await screen.findByText("Loaded media");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.body.style.overflow).toBe("");
  });
  it("keeps the failed lightbox dialog during exit and offers Close", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const Media = lazy(() => Promise.reject(new Error("chunk failed")));
    const [open, setOpen] = createSignal(true);
    render(() => (
      <LanguageProvider>
        <UiLayerProvider>
          <LightboxBoundary
            open={open()}
            name="synthetic.png"
            resetKey={1}
            onClose={() => setOpen(false)}
          >
            <Media />
          </LightboxBoundary>
        </UiLayerProvider>
      </LanguageProvider>
    ));
    await screen.findByRole("alert");
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAccessibleName(i18n.t("lazyLoad.lightboxFailed"));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    flush();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(dialog).toBeInTheDocument();
    expect(document.body.style.overflow).toBe("");
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
  });
});
