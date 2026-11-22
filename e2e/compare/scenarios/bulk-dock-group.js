// The control seed with the first three tiles selected in bulk mode (the clicks of
// gallery-selection), the dock on its Group & order tab and the group key "trip" typed: the tall
// dock shows the key row and the inline order board of the three selected tiles. Nothing is saved.
export default {
  id: "bulk-dock-group",
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
    ["wait", "idle"]
  ]
};
