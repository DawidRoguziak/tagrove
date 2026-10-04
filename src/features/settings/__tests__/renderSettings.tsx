import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import type { JSX } from "@solidjs/web";
import { LanguageProvider } from "../../../i18n/solid";
import { UiLayerProvider } from "../../../ui/overlays/UiLayerProvider";
import { settle } from "../../../test/settle";

export function renderSettings(element: () => JSX.Element) {
  const [content, setContent] = createSignal<() => JSX.Element>(() => element);
  const view = render(() => (
    <LanguageProvider>
      <UiLayerProvider>{content()()}</UiLayerProvider>
    </LanguageProvider>
  ));
  return { ...view, rerender: (next: () => JSX.Element) => settle(() => setContent(() => next)) };
}

import { createSettings } from "../createSettings";
import { SettingsPanel } from "../SettingsPanel";
import { ThumbnailStore } from "../../../library/thumbnails/thumbnailStore";
export function TestSettingsPanel() {
  const settings = createSettings({
    setLoading: () => {},
    refreshLibrary: async () => {},
    onRootRemoved: () => {},
    onImportDbRestored: () => {},
    onLibraryCleared: () => {},
    onTagCacheInvalidated: () => {},
    runWithTagMutationBarrier: (operation) => operation()
  });
  return (
    <SettingsPanel
      {...settings}
      highlightScanSection={false}
      appearance={{
        theme: "light",
        onThemeChange: () => {},
        language: "en",
        onLanguageChange: () => {}
      }}
      thumbnailStore={new ThumbnailStore()}
      onQueueThumbnailsByIds={() => {}}
    />
  );
}
