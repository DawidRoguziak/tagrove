import { useEffect, useState } from "react";
import { applyTheme, readInitialTheme } from "../components/app/services/themeService";

export function useAppTheme() {
  const [theme, setTheme] = useState(readInitialTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  return {
    theme,
    setTheme
  };
}
