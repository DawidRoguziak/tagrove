// The state of bulk-dock-group with "Sort in larger view" pressed: the order board of the three
// selected tiles in its dialog, over the tall dock whose inline board is empty meanwhile. Nothing
// is saved.
export default {
  id: "group-order-modal",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Enable bulk actions"]'],
    ["wait", 'button[aria-label="Disable bulk actions"]'],
    ["click", 'button[data-asset-index="1"]'],
    ["modclick", "Control", 'button[data-asset-index="0"]'],
    ["modclick", "Shift", 'button[data-asset-index="2"]'],
    ["wait", 'button[data-asset-index="2"][aria-pressed="true"]'],
    ["wait", '[data-testid="bulk-action-panel"][data-selected-count="3"]'],
    ["click", ".workspace-count"],
    ["click", '[data-testid="bulk-tab-group"]'],
    ["wait", '[data-testid="bulk-group-panel"]'],
    ["fill", "#bulk-group-key-input", "trip"],
    ["wait", '[data-testid="bulk-inline-order-grid"]'],
    ["click", '[data-testid="bulk-open-order-modal"]'],
    ["wait", '[data-testid="bulk-order-modal"]'],
    ["wait", "idle"]
  ]
};
