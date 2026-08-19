import { lazy, Suspense, useEffect } from "react";
import { AppGalleryView } from "./components/app/AppGalleryView";
import { useAppShellController } from "./components/app/hooks/useAppShellController";

const AppSettingsView = lazy(() =>
  import("./components/app/AppSettingsView").then((module) => ({ default: module.AppSettingsView }))
);
const LightboxModal = lazy(() =>
  import("./components/lightbox/LightboxModal").then((module) => ({ default: module.LightboxModal }))
);

export default function App() {
  const controller = useAppShellController();

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void Promise.all([
        import("./components/app/AppSettingsView"),
        import("./components/lightbox/LightboxModal")
      ]);
    }, 1500);
    return () => window.clearTimeout(timer);
  }, []);

  return (
    <main
      ref={controller.appScrollRef}
      className="gallery-scroll h-full min-h-0 overflow-x-hidden overflow-y-auto"
    >
      {controller.settingsViewOpen ? (
        <Suspense fallback={null}>
          <AppSettingsView {...controller.settingsView} />
        </Suspense>
      ) : (
        <AppGalleryView
          search={controller.galleryView.search}
          media={controller.galleryView.media}
          bulkSelection={controller.galleryView.bulkSelection}
          onOpenSettingsView={controller.galleryView.onOpenSettingsView}
        />
      )}

      <Suspense fallback={null}>
        {controller.lightbox.selected ? <LightboxModal {...controller.lightbox} /> : null}
      </Suspense>
    </main>
  );
}
