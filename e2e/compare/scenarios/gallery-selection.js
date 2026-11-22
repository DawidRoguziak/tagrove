// The control seed with three tiles selected in bulk mode: a click on the second tile, a
// Control-click on the first (which becomes the anchor) and a Shift-click on the third, which
// replaces the selection with the range from the anchor. The last click on the item count
// takes the pointer's hover off the tiles.
export default {
  id: "gallery-selection",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Enable bulk actions"]'],
    ["wait", 'button[aria-label="Disable bulk actions"]'],
    ["click", 'button[data-asset-index="1"]'],
    ["modclick", "Control", 'button[data-asset-index="0"]'],
    ["modclick", "Shift", 'button[data-asset-index="2"]'],
    ["wait", 'button[data-asset-index="2"][aria-pressed="true"]'],
    ["click", ".workspace-count"]
  ]
};
