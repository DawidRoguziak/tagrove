// The settings page in the light theme, scrolled by its navigation to the appearance section:
// the theme cards with Light chosen and the language select below them.
export default {
  id: "settings-appearance-language",
  theme: "light",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", 'nav a[href="#settings-appearance"]'],
    ["wait", 'nav a[href="#settings-appearance"][aria-current="location"]']
  ]
};
