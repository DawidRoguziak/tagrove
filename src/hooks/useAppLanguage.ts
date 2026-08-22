import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import type { AppLanguage } from "../components/app/types";
import { changeAppLanguage, getActiveLanguage } from "../i18n";

export function useAppLanguage() {
  useTranslation();

  const setLanguage = useCallback((nextLanguage: AppLanguage) => {
    void changeAppLanguage(nextLanguage).catch((error) => {
      console.error("Changing the application language failed", error);
    });
  }, []);

  return {
    language: getActiveLanguage(),
    setLanguage
  };
}
