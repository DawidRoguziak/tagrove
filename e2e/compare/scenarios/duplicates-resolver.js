// The duplicate resolver open on the seed's one group (control-1.png in the root and in its
// subfolder): the heading with the counts and the four actions, the scan's summary, the queue
// hint, and the group card with a thumbnail, the path, the rename field and the actions of each
// file. Nothing is staged. The last click parks the pointer on the dialog's heading.
export default {
  id: "duplicates-resolver",
  theme: "dark",
  seed: "duplicates",
  tauri: [
    ["click", 'button[aria-label="Open settings"]'],
    ["wait", '[data-testid="scan-settings-section"]'],
    ["click", 'nav a[href="#settings-duplicates"]'],
    ["click", '//*[@id="settings-duplicates"]//button[normalize-space(.)="Start"]'],
    ["wait", '[role="dialog"][aria-labelledby="duplicate-resolver-heading"]'],
    ["wait", '//*[@role="dialog"]//h4[normalize-space(.)="control-1.png"]'],
    ["click", "#duplicate-resolver-heading"]
  ]
};
