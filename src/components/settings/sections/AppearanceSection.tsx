import { useTranslation } from "react-i18next";
import type { AppLanguage } from "../../app/types";
import { APP_LANGUAGES, APP_LANGUAGE_NATIVE_LABELS } from "../../../i18n/languages";

interface AppearanceSectionProps {
  theme: "light" | "dark";
  onThemeChange: (value: "light" | "dark") => void;
  language: AppLanguage;
  onLanguageChange: (value: AppLanguage) => void;
}

export function AppearanceSection({
  theme,
  onThemeChange,
  language,
  onLanguageChange
}: AppearanceSectionProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-5 shadow-[var(--shadow-surface)] sm:p-6 lg:grid-cols-[minmax(220px,0.8fr)_1.2fr] lg:items-start">
      <div>
        <h2 className="m-0 text-lg">{t("settings.appearance.heading")}</h2>
        <p className="m-0 mt-1 text-sm leading-relaxed text-base-content/60">{t("settings.appearance.description")}</p>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="grid gap-1.5">
          <label htmlFor="settings-theme-select" className="text-xs font-semibold text-base-content/68">
            {t("settings.appearance.theme.label")}
          </label>
          <select
            id="settings-theme-select"
            className="theme-select h-11 min-h-11 w-full text-sm"
            aria-label={t("settings.appearance.theme.ariaLabel")}
            value={theme}
            onChange={(event) => onThemeChange(event.target.value as "light" | "dark")}
          >
            <option value="light">{t("settings.appearance.theme.light")}</option>
            <option value="dark">{t("settings.appearance.theme.dark")}</option>
          </select>
        </div>

        <div className="grid gap-1.5">
          <label htmlFor="settings-language-select" className="text-xs font-semibold text-base-content/68">
            {t("settings.appearance.language.label")}
          </label>
          <select
            id="settings-language-select"
            className="theme-select h-11 min-h-11 w-full text-sm"
            aria-label={t("settings.appearance.language.ariaLabel")}
            value={language}
            onChange={(event) => onLanguageChange(event.target.value as AppLanguage)}
          >
            {APP_LANGUAGES.map((languageCode) => (
              <option key={languageCode} value={languageCode}>
                {APP_LANGUAGE_NATIVE_LABELS[languageCode]}
              </option>
            ))}
          </select>
        </div>
      </div>
    </section>
  );
}
