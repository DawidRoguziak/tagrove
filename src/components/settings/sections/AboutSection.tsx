import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version, license } from "../../../../package.json";
import { developerName, repository } from "../../../../packaging/flatpak/publisher.json";
import licenseText from "../../../../LICENSE?raw";
import privacyText from "../../../../PRIVACY.md?raw";
import { UiButton } from "../../UI/UiButton";

const links = [
  { label: "source", url: repository },
  { label: "contact", url: `${repository}/issues` }
] as const;

export function AboutSection() {
  const { t } = useTranslation();
  const [opening, setOpening] = useState(false);
  const [failedUrl, setFailedUrl] = useState<string | null>(null);

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
    <section aria-labelledby="settings-about-heading" className="grid min-w-0 gap-5 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-surface)]">
      <div>
        <h2 id="settings-about-heading" className="m-0 text-lg">{t("settings.about.heading")}</h2>
        <p className="m-0 mt-1 text-xl font-semibold">Tagrove</p>
      </div>
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-5 gap-y-2 text-sm">
        <dt className="text-[var(--text-muted)]">{t("settings.about.version")}</dt>
        <dd className="m-0" data-testid="about-version">{version}</dd>
        <dt className="text-[var(--text-muted)]">{t("settings.about.publisher")}</dt>
        <dd className="m-0">{developerName}</dd>
        <dt className="text-[var(--text-muted)]">{t("settings.about.license")}</dt>
        <dd className="m-0">{license}</dd>
      </dl>
      <div className="flex flex-wrap gap-2">
        {links.map(({ label, url }) => (
          <UiButton key={label} disabled={opening} onClick={() => void openLink(url)}>
            {t(`settings.about.${label}`)}
          </UiButton>
        ))}
      </div>
      {failedUrl ? (
        <div role="alert" className="grid gap-2 text-sm text-error">
          <p className="m-0">{t("settings.about.openError")}</p>
          <input className="w-full text-sm" aria-label={t("settings.about.linkAddress")} readOnly value={failedUrl} />
        </div>
      ) : null}
      <p className="m-0 text-sm leading-relaxed text-[var(--text-muted)]">{t("settings.about.rights")}</p>
      <p className="m-0 text-sm leading-relaxed text-[var(--text-muted)]">{t("settings.about.fileSafety")}</p>
      <details className="min-w-0 rounded-[var(--radius-control)] border border-[var(--border-soft)] p-3">
        <summary className="cursor-pointer text-sm font-semibold">{t("settings.about.fullLicense")}</summary>
        <pre lang="en" className="m-0 mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed" tabIndex={0}>{licenseText}</pre>
      </details>
      <details className="min-w-0 rounded-[var(--radius-control)] border border-[var(--border-soft)] p-3">
        <summary className="cursor-pointer text-sm font-semibold">{t("settings.about.privacy")}</summary>
        <pre lang="en" className="m-0 mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words text-xs leading-relaxed" tabIndex={0}>{privacyText}</pre>
      </details>
    </section>
  );
}
