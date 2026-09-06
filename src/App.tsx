import { ThumbnailContext } from "./components/UI/ThumbnailSubscription";
import { SuggestionProvider } from "./components/search/worker/SuggestionProvider";
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { AppGalleryView } from "./components/app/AppGalleryView";
import { useAppShellController } from "./components/app/hooks/useAppShellController";
import { UiLayerProvider, useUiLayer } from "./components/UI/UiLayerProvider";
import { LazyErrorBoundary } from "./components/UI/LazyErrorBoundary";
import { UiAlert } from "./components/UI/UiAlert";
import { UiButton } from "./components/UI/UiButton";
import { UiModal } from "./components/UI/UiModal";
import { useTranslation } from "react-i18next";

const AppSettingsView = lazy(() =>
  import("./components/app/AppSettingsView").then((module) => ({ default: module.AppSettingsView }))
);
const LightboxModal = lazy(() =>
  import("./components/lightbox/LightboxModal").then((module) => ({ default: module.LightboxModal }))
);

function SettingsViewLayer({ children, onBack }: { children: ReactNode; onBack: () => void }) {
  const layerRef = useRef<HTMLDivElement | null>(null);
  useUiLayer({
    active: true,
    modal: false,
    containerRef: layerRef,
    onEscape: onBack,
    getRestoreFocus: () => document.getElementById("open-settings-button")
  });

  return <div ref={layerRef} className="min-h-full">{children}</div>;
}

function AppContent() {
  const controller = useAppShellController();
  const { t } = useTranslation();
  const lightboxRestoreFocusRef = useRef<HTMLElement | null>(null);
  const previousSelectedIdRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    const selectedId = controller.lightbox.selected?.id ?? null;
    if (selectedId !== null && previousSelectedIdRef.current === null) {
      lightboxRestoreFocusRef.current = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    }
    previousSelectedIdRef.current = selectedId;
  }, [controller.lightbox.selected?.id]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void Promise.allSettled([
        import("./components/app/AppSettingsView"),
        import("./components/lightbox/LightboxModal")
      ]);
    }, 1500);
    return () => window.clearTimeout(timer);
  }, []);

  return (
    <ThumbnailContext.Provider value={controller.galleryView.media.thumbnailStore ?? null}><main
      className={controller.settingsViewOpen
        ? "gallery-scroll h-full min-h-0 overflow-x-hidden overflow-y-auto"
        : "flex h-full min-h-0 flex-col overflow-hidden"}
    >
      {controller.settingsViewOpen ? (
        <SettingsViewLayer onBack={controller.settingsView.onBack}>
          <LazyErrorBoundary
            resetKey={controller.settingsViewOpen}
            fallback={
              <div className="mx-auto grid min-h-full w-full max-w-[1180px] content-center gap-4 px-4 py-8">
                <UiAlert tone="error" title={t("lazyLoad.settingsFailed")}>
                  {t("lazyLoad.retryDescription")}
                </UiAlert>
                <div className="flex gap-2">
                  <UiButton onClick={() => window.location.reload()}>{t("gallery.retry")}</UiButton>
                  <UiButton variant="ghost" onClick={controller.settingsView.onBack}>{t("common.back")}</UiButton>
                </div>
              </div>
            }
          >
            <Suspense fallback={<p className="p-6" role="status">{t("common.loading")}</p>}>
              <AppSettingsView {...controller.settingsView} />
            </Suspense>
          </LazyErrorBoundary>
        </SettingsViewLayer>
      ) : (
        <AppGalleryView
          search={controller.galleryView.search}
          media={controller.galleryView.media}
          bulkSelection={controller.galleryView.bulkSelection}
          onOpenSettingsView={controller.galleryView.onOpenSettingsView}
        />
      )}

      <LazyErrorBoundary
        resetKey={controller.lightbox.selected?.id ?? null}
        fallback={
          <UiModal
            open={Boolean(controller.lightbox.selected)}
            onClose={controller.lightbox.onClose}
            getRestoreFocus={() => lightboxRestoreFocusRef.current}
            ariaLabel={t("lazyLoad.lightboxFailed")}
            size="small"
          >
            <UiAlert tone="error" title={t("lazyLoad.lightboxFailed")}>
              {t("lazyLoad.retryDescription")}
            </UiAlert>
            <div className="mt-4 flex gap-2">
              <UiButton onClick={() => window.location.reload()}>{t("gallery.retry")}</UiButton>
              <UiButton variant="ghost" onClick={controller.lightbox.onClose}>{t("common.close")}</UiButton>
            </div>
          </UiModal>
        }
      >
        <Suspense fallback={
          <UiModal
            open={Boolean(controller.lightbox.selected)}
            onClose={controller.lightbox.onClose}
            ariaLabel={t("lightbox.previewDialog", {
              name: controller.lightbox.selected?.file_name ?? ""
            })}
            size="small"
          >
            <p className="m-0 text-center" role="status">{t("common.loading")}</p>
          </UiModal>
        }>
          {controller.lightbox.selected ? (
            <LightboxModal
              {...controller.lightbox}
              getRestoreFocus={() => lightboxRestoreFocusRef.current}
            />
          ) : null}
        </Suspense>
      </LazyErrorBoundary>
    </main></ThumbnailContext.Provider>
  );
}

export default function App() {
  return (
    <UiLayerProvider>
      <SuggestionProvider><AppContent /></SuggestionProvider>
    </UiLayerProvider>
  );
}
