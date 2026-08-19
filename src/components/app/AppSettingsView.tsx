import { UiButton } from "../UI/UiButton";
import { UiIcon } from "../UI/UiIcon";
import { SettingsPanel } from "../settings/SettingsPanel";
import type { SettingsPanelProps } from "../settings/SettingsPanel";
import { useTranslation } from "react-i18next";

interface AppSettingsViewProps extends Omit<SettingsPanelProps, "fullView"> {
  onBack: () => void;
}

export function AppSettingsView({ onBack, ...settingsPanelProps }: AppSettingsViewProps) {
  const { t } = useTranslation();

  return (
    <div className="grid min-h-full content-start pb-8">
      <header className="sticky top-0 z-40 border-b border-[var(--border-soft)] bg-[var(--window-chrome-bg)] px-4 py-3 backdrop-blur-xl sm:px-6">
        <div className="mx-auto flex w-full max-w-[1180px] items-center gap-4">
          <UiButton
            variant="ghost"
            onClick={onBack}
            className="z-40 flex items-center gap-2 px-3"
            aria-label={t("common.back")}
            title={t("common.back")}
          >
            <UiIcon name="arrow-left" className="h-4 w-4" />
            {t("common.back")}
          </UiButton>
          <div className="min-w-0 border-l border-[var(--border-soft)] pl-4">
            <h1 className="m-0 text-xl leading-tight sm:text-2xl">{t("settings.page.heading")}</h1>
            <p className="m-0 mt-0.5 hidden text-xs text-base-content/60 sm:block">
              {t("settings.page.description")}
            </p>
          </div>
        </div>
      </header>

      <div className="pt-5 sm:pt-7">
        <SettingsPanel fullView {...settingsPanelProps} />
      </div>
    </div>
  );
}
