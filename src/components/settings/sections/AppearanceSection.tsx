import { useId } from "react";
import { useTranslation } from "react-i18next";

interface AppearanceSectionProps {
  theme: "light" | "dark";
  onThemeChange: (value: "light" | "dark") => void;
}

export function AppearanceSection({
  theme,
  onThemeChange
}: AppearanceSectionProps) {
  const { t } = useTranslation();
  const themeName = useId();

  return (
    <section className="grid gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)] lg:grid-cols-[minmax(220px,0.8fr)_1.2fr] lg:items-start">
      <div>
        <h2 className="m-0 text-base">{t("settings.appearance.heading")}</h2>
        <p className="m-0 mt-1 text-sm leading-relaxed text-[var(--text-muted)]">{t("settings.appearance.description")}</p>
      </div>
      <div className="grid items-start gap-5 sm:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <fieldset className="theme-choices" aria-label={t("settings.appearance.theme.ariaLabel")}>
          <legend className="text-xs font-semibold text-[var(--text-muted)]">{t("settings.appearance.theme.label")}</legend>
          {(["light", "dark"] as const).map((choice) => (
            <label className="theme-choice" key={choice}>
              <input className="choice-input" type="radio" name={themeName} value={choice}
                checked={theme === choice} onChange={() => onThemeChange(choice)} />
              <span>
                <span className={`theme-preview theme-preview--${choice}`} aria-hidden="true"><i /><b /><em /></span>
                {t(`settings.appearance.theme.${choice}`)}
              </span>
            </label>
          ))}
        </fieldset>
      </div>
    </section>
  );
}
