import { For } from "solid-js";
import { useLanguage } from "../../../i18n/solid";
import {
  APP_LANGUAGES,
  APP_LANGUAGE_NATIVE_LABELS,
  type AppLanguage
} from "../../../i18n/languages";

interface LanguageSectionProps {
  language: AppLanguage;
  onLanguageChange: (value: AppLanguage) => void;
}

export function LanguageSection(props: LanguageSectionProps) {
  const { t } = useLanguage();

  return (
    <section class="grid gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <div>
        <h2 class="m-0 text-lg">
          <label for="settings-language-select">{t("settings.language.heading")}</label>
        </h2>
        <p class="m-0 mt-1 text-sm leading-relaxed text-[var(--text-muted)]">
          {t("settings.language.description")}
        </p>
      </div>
      <select
        id="settings-language-select"
        class="theme-select h-9 min-h-9 w-full max-w-[480px] text-sm"
        aria-label={t("settings.language.ariaLabel")}
        value={props.language}
        onChange={(event) => {
          const selected = APP_LANGUAGES.find((code) => code === event.currentTarget.value);
          if (selected) props.onLanguageChange(selected);
        }}
      >
        <For each={APP_LANGUAGES}>
          {(languageCode) => (
            <option value={languageCode}>{APP_LANGUAGE_NATIVE_LABELS[languageCode]}</option>
          )}
        </For>
      </select>
    </section>
  );
}
