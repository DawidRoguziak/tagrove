import { useCallback, useEffect, useState } from "react";

export function useSettingsView() {
  const [settingsViewOpen, setSettingsViewOpen] = useState(false);

  const openSettingsView = useCallback(() => {
    setSettingsViewOpen(true);
  }, []);

  const closeSettingsView = useCallback(() => {
    setSettingsViewOpen(false);
  }, []);

  useEffect(() => {
    if (!settingsViewOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }

      setSettingsViewOpen(false);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [settingsViewOpen]);

  return {
    settingsViewOpen,
    openSettingsView,
    closeSettingsView
  };
}
