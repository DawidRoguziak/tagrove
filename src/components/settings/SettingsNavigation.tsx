import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { UiIcon, type UiIconName } from "../UI/UiIcon";

const sections = [
  { id: "scan", label: "settings.scan.heading", icon: "folder" },
  { id: "appearance", label: "settings.appearance.heading", icon: "settings" },
  { id: "language", label: "settings.language.heading", icon: "settings" },
  { id: "import-export", label: "settings.importExport.heading", icon: "copy" },
  { id: "duplicates", label: "settings.duplicates.heading", icon: "group" },
  { id: "danger", label: "settings.danger.heading", icon: "trash" },
  { id: "about", label: "settings.about.heading", icon: "info" }
] satisfies { id: string; label: string; icon: UiIconName }[];

export function SettingsNavigation() {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState("scan");

  useEffect(() => {
    const elements = sections.flatMap(({ id }) => {
      const element = document.getElementById(`settings-${id}`);
      return element ? [element] : [];
    });
    const root = elements[0]?.closest("main");
    if (!root) return;
    const update = () => {
      if (root.scrollHeight > root.clientHeight && root.scrollTop + root.clientHeight >= root.scrollHeight - 1) {
        setActiveSection(sections[sections.length - 1].id);
        return;
      }
      const top = root.getBoundingClientRect().top + 120;
      const current = elements.reduce<HTMLElement | undefined>((previous, element) =>
        element.getBoundingClientRect().top <= top ? element : previous, undefined);
      setActiveSection(current?.id.replace("settings-", "") ?? "scan");
    };
    root.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    update();
    return () => {
      root.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, []);

  return (
    <nav className="settings-nav" aria-label={t("workspace.settingsNavigation")}>
      {sections.map(({ id, label, icon }) => (
        <a key={id} href={`#settings-${id}`} aria-current={activeSection === id ? "location" : undefined}
          onClick={(event) => {
            event.preventDefault();
            const target = document.getElementById(`settings-${id}`);
            target?.focus({ preventScroll: true });
            target?.scrollIntoView({ block: "start", behavior: "instant" });
            setActiveSection(id);
          }}>
          <UiIcon name={icon} className="h-4 w-4 shrink-0" />
          {t(label)}
        </a>
      ))}
    </nav>
  );
}
