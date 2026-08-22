import { useCallback, useState } from "react";

export function useSettingsView() {
  const [settingsViewOpen, setSettingsViewOpen] = useState(false);

  const openSettingsView = useCallback(() => {
    setSettingsViewOpen(true);
  }, []);

  const closeSettingsView = useCallback(() => {
    setSettingsViewOpen(false);
  }, []);

  return {
    settingsViewOpen,
    openSettingsView,
    closeSettingsView
  };
}
