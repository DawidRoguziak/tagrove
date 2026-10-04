import { useLanguage } from "../i18n/solid";
import { SettingsNavigation } from "../features/settings/SettingsNavigation";
import { AppearanceSection } from "../features/settings/sections/AppearanceSection";
import { LanguageSection } from "../features/settings/sections/LanguageSection";
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
        <section class="min-h-0 px-4 pb-4 sm:px-6">
          <div class="settings-card settings-layout">
            <SettingsNavigation />
            <div class="grid min-w-0 content-start gap-5">
              <div id="settings-appearance" class="settings-section" tabindex={-1}>
                <AppearanceSection theme={runtime.theme()} onThemeChange={runtime.setTheme} />
              </div>
              <div id="settings-language" class="settings-section" tabindex={-1}>
                <LanguageSection
                  language={language.language()}
                  onLanguageChange={(value) => {
                    void language
                      .setLanguage(value)
                      .catch((error) =>
                        console.error("Changing the application language failed", error)
                      );
                  }}
                />
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
