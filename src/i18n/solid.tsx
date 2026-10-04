import { createContext, createSignal, onCleanup, useContext } from "solid-js";
import type { JSX } from "@solidjs/web";
import type { TOptions } from "i18next";
import i18n, { changeAppLanguage, getActiveLanguage } from "./core";

export function createLanguage() {
  const [language, setLanguage] = createSignal(getActiveLanguage());
  const changed = () => setLanguage(getActiveLanguage());
  i18n.on("languageChanged", changed);
  onCleanup(() => i18n.off("languageChanged", changed));
  return {
    language,
    setLanguage: changeAppLanguage,
    t: (key: string, options?: Omit<TOptions, "context"> & { context?: string }): string => {
      language();
      return i18n.t(key, options);
    }
  };
}
const LanguageContext = createContext<ReturnType<typeof createLanguage>>();
export function LanguageProvider(props: { children: JSX.Element }) {
  const language = createLanguage();
  return <LanguageContext value={language}>{props.children}</LanguageContext>;
}
export function useLanguage() {
  const language = useContext(LanguageContext);
  if (!language) throw new Error("LanguageProvider is required");
  return language;
}
