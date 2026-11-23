// The settings page scrolled by its navigation's About link to its end: the danger zone with its
// clear button and status line, and the About section below it (version, publisher, license, the
// two links and the folded legal texts).
export default {
  id: "settings-danger-about",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", 'nav a[href="#settings-about"]'],
    ["wait", 'nav a[href="#settings-about"][aria-current="location"]']
  ]
};
