import { createSignal, For } from "solid-js";
import { useLanguage } from "../../../i18n/solid";
import { openUrl } from "../../../platform/tauri/dialogs";
import { version, license } from "../../../../package.json";
import { developerName, repository } from "../../../../packaging/flatpak/publisher.json";
import licenseText from "../../../../LICENSE?raw";
import privacyText from "../../../../PRIVACY.md?raw";
import { UiButton } from "../../../ui/controls/UiButton";
const links = [
  { label: "source", url: repository },
  { label: "contact", url: `${repository}/issues` }
] as const;
export function AboutSection() {
  const { t } = useLanguage();
  const [opening, setOpening] = createSignal(false);
  const [failedUrl, setFailedUrl] = createSignal<string | null>(null);
  async function openLink(url: string) {
    setOpening(true);
    setFailedUrl(null);
    try {
      await openUrl(url);
    } catch {
      setFailedUrl(url);
    } finally {
      setOpening(false);
    }
  }
  return (
    <section
      aria-labelledby="settings-about-heading"
      class="grid min-w-0 gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]"
    >
      <div>
        <h2 id="settings-about-heading" class="m-0 text-lg">
          {t("settings.about.heading")}
        </h2>
        <p class="m-0 mt-1 text-xl font-semibold">Tagrove</p>
      </div>
      <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-5 gap-y-2 text-sm">
        <dt class="text-[var(--text-muted)]">{t("settings.about.version")}</dt>
        <dd class="m-0" data-testid="about-version">
          {version}
        </dd>
        <dt class="text-[var(--text-muted)]">{t("settings.about.publisher")}</dt>
        <dd class="m-0">{developerName}</dd>
        <dt class="text-[var(--text-muted)]">{t("settings.about.license")}</dt>
        <dd class="m-0">{license}</dd>
      </dl>
      <div class="flex flex-wrap gap-2">
        <For each={links}>
          {({ label, url }) => (
            <UiButton disabled={opening()} onClick={() => void openLink(url)}>
              {t(`settings.about.${label}`)}
            </UiButton>
          )}
        </For>
      </div>
      {failedUrl() ? (
        <div role="alert" class="grid gap-2 text-sm text-error">
          <p class="m-0">{t("settings.about.openError")}</p>
          <input
            class="w-full text-sm"
            aria-label={t("settings.about.linkAddress")}
            readonly
            value={failedUrl() ?? ""}
          />
        </div>
      ) : null}
      <p class="m-0 text-sm leading-relaxed text-[var(--text-muted)]">
        {t("settings.about.rights")}
      </p>
      <p class="m-0 text-sm leading-relaxed text-[var(--text-muted)]">
        {t("settings.about.fileSafety")}
      </p>
      <details class="min-w-0 rounded-[var(--radius-control)] border border-[var(--border-soft)] p-3">
        <summary class="cursor-pointer text-sm font-semibold">
          {t("settings.about.fullLicense")}
        </summary>
        <pre
          lang="en"
          class="m-0 mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed"
          tabindex={0}
        >
          {licenseText}
        </pre>
      </details>
      <details class="min-w-0 rounded-[var(--radius-control)] border border-[var(--border-soft)] p-3">
        <summary class="cursor-pointer text-sm font-semibold">
          {t("settings.about.privacy")}
        </summary>
        <pre
          lang="en"
          class="m-0 mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed"
          tabindex={0}
        >
          {privacyText}
        </pre>
      </details>
    </section>
  );
}
