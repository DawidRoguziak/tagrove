import { useTranslation } from "react-i18next";
import { APP_LANGUAGES, APP_LANGUAGE_NATIVE_LABELS, type AppLanguage } from "../../../i18n/languages";

interface LanguageSectionProps {
  language: AppLanguage;
  onLanguageChange: (value: AppLanguage) => void;
}

export function LanguageSection({ language, onLanguageChange }: LanguageSectionProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)] lg:grid-cols-[minmax(220px,0.8fr)_1.2fr] lg:items-center">
      <div>
        <h2 className="m-0 text-base">
          <label htmlFor="settings-language-select">{t("settings.language.heading")}</label>
        </h2>
        <p className="m-0 mt-1 text-sm leading-relaxed text-[var(--text-muted)]">{t("settings.language.description")}</p>
      </div>
      <select
        id="settings-language-select"
        className="theme-select h-9 min-h-9 w-full text-sm"
        aria-label={t("settings.language.ariaLabel")}
        value={language}
        onChange={(event) => {
          const selected = APP_LANGUAGES.find((code) => code === event.target.value);
          if (selected) onLanguageChange(selected);
        }}
      >
        {APP_LANGUAGES.map((languageCode) => (
          <option key={languageCode} value={languageCode}>
            {APP_LANGUAGE_NATIVE_LABELS[languageCode]}
          </option>
        ))}
      </select>
    </section>
  );
}
