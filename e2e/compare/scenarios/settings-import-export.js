// The settings page scrolled by its navigation to the import/export section: the CSV and the
// backup rows with their two buttons each, the section's ready line and the warning below it.
export default {
  id: "settings-import-export",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", 'nav a[href="#settings-import-export"]'],
    ["wait", 'nav a[href="#settings-import-export"][aria-current="location"]']
  ]
};
