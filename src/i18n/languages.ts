export const APP_LANGUAGES = [
  "en",
  "pl",
  "fr",
  "de",
  "it",
  "es",
  "ru",
  "zh",
  "ja",
  "ko",
  "cs"
] as const;

export type AppLanguage = (typeof APP_LANGUAGES)[number];

export const APP_LANGUAGE_NATIVE_LABELS: Record<AppLanguage, string> = {
  en: "English",
  pl: "Polski",
  fr: "Français",
  de: "Deutsch",
  it: "Italiano",
  es: "Español",
  ru: "Русский",
  zh: "中文（简体）",
  ja: "日本語",
  ko: "한국어",
  cs: "Čeština"
};
