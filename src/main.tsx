import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import App from "./App";
import "./styles.css";
import { applyTheme, THEME_STORAGE_KEY } from "./components/app/services/themeService";

const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
const initialTheme = storedTheme === "light" || storedTheme === "dark"
  ? storedTheme
  : "dark";

applyTheme(initialTheme);

if (import.meta.env.VITE_MEDIATAGGER_PERF === "1" && "PerformanceObserver" in window) {
  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        console.info(`[perf] name=${entry.name} duration_ms=${entry.duration.toFixed(1)}`);
      }
    });
    observer.observe({ entryTypes: ["longtask", "measure"] });
  } catch {
    // WebView versions without Long Tasks support keep running without instrumentation.
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
