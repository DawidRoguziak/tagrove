import { useLanguage } from "../i18n/solid";
import { SettingsPanel } from "../features/settings/SettingsPanel";
import { UiButton } from "../ui/controls/UiButton";
import { UiIcon } from "../ui/controls/UiIcon";
import { WindowControls } from "./WindowControls";
import { useAppRuntime } from "./AppRuntimeContext";
export default function AppSettingsView() {
  const runtime = useAppRuntime();
  const language = useLanguage();
  const { t } = language;
  return (
    <div class="grid min-h-full grid-cols-1 content-start pb-8">
      <header
        data-tauri-drag-region="deep"
        class="window-drag-surface sticky top-0 z-40 border-b border-[var(--border-soft)] bg-[var(--window-chrome-bg)] px-4 py-3 sm:px-6"
      >
        <div class="flex w-full items-center gap-4">
          <UiButton
            variant="ghost"
            onClick={runtime.closeSettings}
            className="z-40 flex items-center gap-2 px-3"
            aria-label={t("common.back")}
            title={t("common.back")}
          >
            <UiIcon name="arrow-left" className="h-4 w-4" />
            {t("common.back")}
          </UiButton>
          <img
            src="/tagrove.svg"
            alt="Tagrove"
            width={28}
            height={28}
            draggable={false}
            class="hidden shrink-0 sm:block"
          />
          <div class="min-w-0 flex-1 border-l border-[var(--border-soft)] pl-4">
            <h1 class="m-0 truncate text-2xl leading-tight">{t("settings.page.heading")}</h1>
            <p class="m-0 mt-0.5 hidden text-xs text-[var(--text-muted)] sm:block">
              {t("settings.page.description")}
            </p>
          </div>
          <WindowControls />
        </div>
      </header>
      <div class="pt-5">
        <SettingsPanel
          highlightScanSection={runtime.highlightScan()}
          appearance={{
            get theme() {
              return runtime.theme();
            },
            onThemeChange: runtime.setTheme,
            get language() {
              return language.language();
            },
            onLanguageChange: (value) => {
              void language
                .setLanguage(value)
                .catch((error) => console.error("Changing the application language failed", error));
            }
          }}
          scan={runtime.settings.scan}
          importExport={runtime.settings.importExport}
          dangerZone={runtime.settings.dangerZone}
          duplicates={runtime.settings.duplicates}
          thumbnailStore={runtime.library.thumbnails.thumbnailStore}
          onQueueThumbnailsByIds={runtime.library.thumbnails.queueThumbnailsByIds}
        />
      </div>
    </div>
  );
}
