import { createEffect, createMemo, lazy, Loading, onCleanup, onSettled, Show } from "solid-js";
import { AppGalleryView } from "./AppGalleryView";
import { LanguageProvider, useLanguage } from "../i18n/solid";
import { UiLayerProvider, createUiLayer } from "../ui/overlays/UiLayerProvider";
import { LazyErrorBoundary } from "../ui/overlays/LazyErrorBoundary";
import { UiAlert } from "../ui/controls/UiAlert";
import { UiButton } from "../ui/controls/UiButton";
import { AppRuntimeProvider, useAppRuntime } from "./AppRuntimeContext";

import LightboxBoundary from "./LightboxBoundary";
const Lightbox = lazy(() => import("../features/lightbox/LightboxModal"));
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
  const lightboxActive = createMemo(() => runtime.lightbox.selected !== null);
  const lightboxActivation = createMemo<{ id: number | null; key: number }>((previous) => {
    const id = runtime.lightbox.selected?.id ?? null;
    return {
      id,
      key: (previous?.key ?? 0) + (id !== null && id !== previous?.id ? 1 : 0)
    };
  });
  let restoreFocus: HTMLElement | null = null;
  createEffect(lightboxActive, (active) => {
    if (active)
      restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  });
  const getRestoreFocus = () => restoreFocus;
  const lightboxLoaded = createMemo<boolean>(
    (previous) => previous || runtime.lightbox.selected !== null
  );
  onSettled(runtime.start);
  const prefetch = setTimeout(() => {
    void Promise.allSettled([
      import("./AppSettingsView"),
      import("../features/lightbox/LightboxModal")
    ]);
  }, 1500);
  onCleanup(() => clearTimeout(prefetch));
  return (
    <>
      <main
        class={
          runtime.view() === "settings"
            ? "gallery-scroll h-full min-h-0 overflow-x-hidden overflow-y-auto"
            : "flex h-full min-h-0 flex-col overflow-hidden"
        }
      >
        <Show when={runtime.view() === "settings"} fallback={<AppGalleryView />}>
          <SettingsView />
        </Show>
      </main>
      <Show when={lightboxLoaded()}>
        <LightboxBoundary
          open={lightboxActive()}
          name={runtime.lightbox.selected?.file_name ?? ""}
          resetKey={lightboxActivation().key}
          getRestoreFocus={getRestoreFocus}
          onClose={runtime.lightbox.close}
        >
          <Lightbox
            getRestoreFocus={getRestoreFocus}
            model={runtime.lightbox}
            client={runtime.suggestions}
            knownTags={runtime.library.vocabulary.knownTags}
          />
        </LightboxBoundary>
      </Show>
    </>
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
