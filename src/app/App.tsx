import { lazy, Loading, onCleanup, Show } from "solid-js";
import { LanguageProvider, useLanguage } from "../i18n/solid";
import { UiLayerProvider, createUiLayer } from "../ui/overlays/UiLayerProvider";
import { LazyErrorBoundary } from "../ui/overlays/LazyErrorBoundary";
import { UiAlert } from "../ui/controls/UiAlert";
import { UiButton } from "../ui/controls/UiButton";
import { UiIcon } from "../ui/controls/UiIcon";
import { AppRuntimeProvider, useAppRuntime } from "./AppRuntimeContext";
import { WindowControls } from "./WindowControls";

const Settings = lazy(() => import("./AppSettingsView"));
function SettingsView() {
  const runtime = useAppRuntime();
  const { t } = useLanguage();
  let container: HTMLDivElement | undefined;
  createUiLayer({
    active: () => true,
    modal: false,
    container: () => container,
    onEscape: runtime.closeSettings,
    getRestoreFocus: () => document.getElementById("open-settings-button")
  });
  return (
    <div
      ref={(el) => {
        container = el;
      }}
      class="min-h-full"
    >
      <LazyErrorBoundary
        resetKey="settings"
        fallback={
          <div class="mx-auto grid min-h-full w-full max-w-[1180px] content-center gap-4 px-4 py-8">
            <UiAlert tone="error" title={t("lazyLoad.settingsFailed")}>
              {t("lazyLoad.retryDescription")}
            </UiAlert>
            <div class="flex gap-2">
              <UiButton onClick={() => window.location.reload()}>{t("gallery.retry")}</UiButton>
              <UiButton variant="ghost" onClick={runtime.closeSettings}>
                {t("common.back")}
              </UiButton>
            </div>
          </div>
        }
      >
        <Loading
          fallback={
            <p class="p-6" role="status">
              {t("common.loading")}
            </p>
          }
        >
          <Settings />
        </Loading>
      </LazyErrorBoundary>
    </div>
  );
}
function AppContent() {
  const runtime = useAppRuntime();
  const { t } = useLanguage();
  const prefetch = setTimeout(() => {
    void Promise.allSettled([import("./AppSettingsView"), import("./LightboxBoundary")]);
  }, 1500);
  onCleanup(() => clearTimeout(prefetch));
  return (
    <main
      class={
        runtime.view() === "settings"
          ? "gallery-scroll h-full min-h-0 overflow-x-hidden overflow-y-auto"
          : "flex h-full min-h-0 flex-col overflow-hidden"
      }
    >
      <Show
        when={runtime.view() === "settings"}
        fallback={
          <>
            <header
              data-tauri-drag-region="deep"
              class="window-drag-surface flex items-center gap-4 border-b border-[var(--border-soft)] bg-[var(--window-chrome-bg)] px-4 py-3 sm:px-6"
            >
              <img src="/tagrove.svg" alt="" width={28} height={28} draggable={false} />
              <h1 class="m-0 flex-1 text-2xl">Tagrove</h1>
              <UiButton
                id="open-settings-button"
                variant="ghost"
                aria-label={t("topBar.openSettings")}
                title={t("topBar.openSettings")}
                onClick={runtime.openSettings}
              >
                <UiIcon name="settings" />
              </UiButton>
              <WindowControls />
            </header>
            <p role="status" class="p-6">
              Solid 2 shell ready
            </p>
          </>
        }
      >
        <SettingsView />
      </Show>
    </main>
  );
}
export default function App() {
  return (
    <LanguageProvider>
      <AppRuntimeProvider>
        <UiLayerProvider>
          <AppContent />
        </UiLayerProvider>
      </AppRuntimeProvider>
    </LanguageProvider>
  );
}
