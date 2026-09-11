import { WindowControls } from "./WindowControls";
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
    <div className="grid min-h-full grid-cols-1 content-start pb-8">
      <header data-tauri-drag-region="deep" className="window-drag-surface sticky top-0 z-40 border-b border-[var(--border-soft)] bg-[var(--window-chrome-bg)] px-4 py-3 sm:px-6">
        <div className="flex w-full items-center gap-4">
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
          <div className="min-w-0 flex-1 border-l border-[var(--border-soft)] pl-4">
            <h1 className="m-0 truncate text-xl leading-tight">{t("settings.page.heading")}</h1>
            <p className="m-0 mt-0.5 hidden text-xs text-[var(--text-muted)] sm:block">
              {t("settings.page.description")}
            </p>
          </div>
          <WindowControls />
        </div>
      </header>

      <div className="pt-5">
        <SettingsPanel fullView {...settingsPanelProps} />
      </div>
    </div>
  );
}
