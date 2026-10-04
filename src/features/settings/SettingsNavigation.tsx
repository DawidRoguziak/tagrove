import { onSettled, createSignal, For } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import { UiIcon, type UiIconName } from "../../ui/controls/UiIcon";

const sections = [
  { id: "appearance", label: "settings.appearance.heading", icon: "palette" },
  { id: "language", label: "settings.language.heading", icon: "languages" }
] satisfies { id: string; label: string; icon: UiIconName }[];

export function SettingsNavigation() {
  const { t } = useLanguage();
  const [activeSection, setActiveSection] = createSignal("appearance");

  onSettled(() => {
    const elements = sections.flatMap(({ id }) => {
      const element = document.getElementById(`settings-${id}`);
      return element ? [element] : [];
    });
    const root = elements[0]?.closest("main");
    if (!root) return;
    const update = () => {
      if (
        root.scrollHeight > root.clientHeight &&
        root.scrollTop + root.clientHeight >= root.scrollHeight - 1
      ) {
        setActiveSection(sections[sections.length - 1].id);
        return;
      }
      const top = root.getBoundingClientRect().top + 120;
      const current = elements.reduce<HTMLElement | undefined>(
        (previous, element) => (element.getBoundingClientRect().top <= top ? element : previous),
        undefined
      );
      setActiveSection(current?.id.replace("settings-", "") ?? "appearance");
    };
    root.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    update();
    return () => {
      root.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  });

  return (
    <nav class="settings-nav" aria-label={t("workspace.settingsNavigation")}>
      <For each={sections}>
        {({ id, label, icon }) => (
          <a
            href={`#settings-${id}`}
            aria-current={activeSection() === id ? "location" : undefined}
            onClick={(event) => {
              event.preventDefault();
              const target = document.getElementById(`settings-${id}`);
              target?.focus({ preventScroll: true });
              target?.scrollIntoView({ block: "start", behavior: "instant" });
              setActiveSection(id);
            }}
          >
            <UiIcon name={icon} className="h-4 w-4 shrink-0" />
            {t(label)}
          </a>
        )}
      </For>
    </nav>
  );
}
