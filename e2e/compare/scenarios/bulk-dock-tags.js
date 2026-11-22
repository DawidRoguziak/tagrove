// The control seed with the first three tiles selected in bulk mode (the clicks of
// gallery-selection) and the tag "holiday" added to them through the dock: the dock is expanded on
// its Tags tab with the added tag as a chip without a remove control. The click on the item count
// before typing takes the pointer's hover off the tiles. A capture shows no caret and no focus
// ring: the window on the private display is not the active one.
export default {
  id: "bulk-dock-tags",
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
    ["wait", "#bulk-tag-draft-input:not([disabled])"],
    ["fill", "#bulk-tag-draft-input", "holiday"],
    ["keys", "Enter"],
    ["wait", '//*[@data-testid="bulk-tag-list"]//span[normalize-space(.)="holiday"]'],
    ["wait", "#bulk-tag-draft-input:not([disabled])"]
  ]
};
