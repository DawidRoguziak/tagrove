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
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    );
  });
  it("switches views, translates live, persists appearance and returns with Escape", async () => {
    render(() => <App />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Open settings" }));
    const language = await screen.findByRole("combobox");
    expect(language.querySelectorAll("option")).toHaveLength(11);
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
