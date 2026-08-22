import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import type { AppLanguage } from "../components/app/types";
import cs from "./locales/cs.json";
import de from "./locales/de.json";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import it from "./locales/it.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import pl from "./locales/pl.json";
import ru from "./locales/ru.json";
import zh from "./locales/zh.json";
import { APP_LANGUAGES } from "./languages";

export const LANGUAGE_STORAGE_KEY = "media-tagger.language";

const FALLBACK_LANGUAGE: AppLanguage = "en";

function normalizeLanguage(value: string | null | undefined): AppLanguage | null {
  if (!value) {
    return null;
  }

  const normalized = value.trim().toLowerCase();
  for (const language of APP_LANGUAGES) {
    if (normalized.startsWith(language)) {
      return language;
    }
  }

  return null;
}

function resolveInitialLanguage(): AppLanguage {
  if (typeof window === "undefined") {
    return FALLBACK_LANGUAGE;
  }

  const stored = normalizeLanguage(window.localStorage.getItem(LANGUAGE_STORAGE_KEY));
  if (stored) {
    return stored;
  }

  return normalizeLanguage(window.navigator.language) ?? FALLBACK_LANGUAGE;
}

const resources = {
  en: { translation: en },
  pl: { translation: pl },
  fr: { translation: fr },
  de: { translation: de },
  it: { translation: it },
  es: { translation: es },
  ru: { translation: ru },
  zh: { translation: zh },
  ja: { translation: ja },
  ko: { translation: ko },
  cs: { translation: cs }
};

void i18n.use(initReactI18next).init({
  resources,
  lng: resolveInitialLanguage(),
  fallbackLng: FALLBACK_LANGUAGE,
  showSupportNotice: false,
  interpolation: {
    escapeValue: false
  }
}).catch((error) => {
  console.error("i18n initialization failed", error);
});

function persistLanguage(language: AppLanguage) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = language;
  }

  if (typeof window !== "undefined") {
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  }
}

export function getActiveLanguage(): AppLanguage {
  return normalizeLanguage(i18n.resolvedLanguage ?? i18n.language) ?? FALLBACK_LANGUAGE;
}

export async function changeAppLanguage(language: AppLanguage): Promise<void> {
  await i18n.changeLanguage(language);
}

persistLanguage(getActiveLanguage());

i18n.on("languageChanged", (nextLanguage: string) => {
  const normalized = normalizeLanguage(nextLanguage) ?? FALLBACK_LANGUAGE;
  persistLanguage(normalized);
});

export default i18n;
