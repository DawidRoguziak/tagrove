// The control seed's one root in the scan section of the settings page, its startup-scan switch
// turned on (a new root starts with it off): the section's status line shows the saved preference. The pointer is parked on the
// section's heading afterwards, so the switch shows no hover state.
export default {
  id: "settings-scan",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", '//*[@data-testid="scan-settings-section"]//li//input[@type="checkbox"]'],
    ["wait", '//*[@data-testid="scan-settings-section"]//*[normalize-space(.)="Saved. Applies on the next app launch."]'],
    ["click", '//*[@data-testid="scan-settings-section"]//h2']
  ]
};
