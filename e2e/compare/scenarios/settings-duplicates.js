// The duplicate section after a scan that found the seed's one group (control-1.png in the root
// and in its subfolder): Start opens the resolver and scans, the resolver is closed again once
// the group is listed, and the section shows the last result and the scan's summary. The last
// click parks the pointer on the section's heading, so no button shows a hover state.
export default {
  id: "settings-duplicates",
  theme: "dark",
  seed: "duplicates",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", 'nav a[href="#settings-duplicates"]'],
    ["click", '//*[@id="settings-duplicates"]//button[normalize-space(.)="Start"]'],
    ["wait", '[role="dialog"][aria-labelledby="duplicate-resolver-heading"]'],
    ["wait", '//*[@role="dialog"]//h4[normalize-space(.)="control-1.png"]'],
    ["click", '//*[@role="dialog"][@aria-labelledby="duplicate-resolver-heading"]//button[normalize-space(.)="Close"]'],
    ["wait", '[role="dialog"][aria-labelledby="duplicate-resolver-heading"]', "gone"],
    ["click", '//*[@id="settings-duplicates"]//h3']
  ]
};
