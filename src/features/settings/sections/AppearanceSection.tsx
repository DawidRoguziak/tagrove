import { createUniqueId, For } from "solid-js";
import { useLanguage } from "../../../i18n/solid";

interface AppearanceSectionProps {
  theme: "light" | "dark";
  onThemeChange: (value: "light" | "dark") => void;
}

export function AppearanceSection(props: AppearanceSectionProps) {
  const { t } = useLanguage();
  const themeName = createUniqueId();

  return (
    <section class="grid gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <div>
        <h2 class="m-0 text-lg">{t("settings.appearance.heading")}</h2>
        <p class="m-0 mt-1 text-sm leading-relaxed text-[var(--text-muted)]">
          {t("settings.appearance.description")}
        </p>
      </div>
      <div class="grid max-w-[480px] items-start gap-5">
        <fieldset class="theme-choices" aria-label={t("settings.appearance.theme.ariaLabel")}>
          <legend class="text-xs font-semibold text-[var(--text-muted)]">
            {t("settings.appearance.theme.label")}
          </legend>
          <For each={["light", "dark"] as const}>
            {(choice) => (
              <label class="theme-choice">
                <input
                  class="choice-input"
                  type="radio"
                  name={themeName}
                  value={choice}
                  checked={props.theme === choice}
                  onChange={() => props.onThemeChange(choice)}
                />
                <span>
                  <span class={`theme-preview theme-preview--${choice}`} aria-hidden="true">
                    <i />
                    <b />
                    <em />
                  </span>
                  {t(`settings.appearance.theme.${choice}`)}
                </span>
              </label>
            )}
          </For>
        </fieldset>
      </div>
    </section>
  );
}
