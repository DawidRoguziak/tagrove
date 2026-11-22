// The control seed with the first tile tagged camera, car and cat through the bulk dock, bulk mode
// off again, and the tag list open from the tools row: camera included with a click, cat excluded
// with a double click, car inactive.
export default {
  id: "tag-list-modal",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[aria-label="Enable bulk actions"]'],
    ["wait", 'button[aria-label="Disable bulk actions"]'],
    ["click", 'button[data-asset-index="0"]'],
    ["wait", "#bulk-tag-draft-input:not([disabled])"],
    ["fill", "#bulk-tag-draft-input", "camera"],
    ["keys", "Enter"],
    ["wait", 'button[aria-label="Remove tag camera"]'],
    ["wait", "#bulk-tag-draft-input:not([disabled])"],
    ["fill", "#bulk-tag-draft-input", "car"],
    ["keys", "Enter"],
    ["wait", 'button[aria-label="Remove tag car"]'],
    ["wait", "#bulk-tag-draft-input:not([disabled])"],
    ["fill", "#bulk-tag-draft-input", "cat"],
    ["keys", "Enter"],
    ["wait", 'button[aria-label="Remove tag cat"]'],
    ["click", 'button[aria-label="Disable bulk actions"]'],
    ["wait", 'button[aria-label="Enable bulk actions"]'],
    ["click", 'button[aria-label="Open tag list"]'],
    ["wait", '[data-testid="tag-list-modal"]'],
    ["click", 'button[aria-label="camera, state: inactive"]'],
    ["wait", 'button[aria-label="camera, state: included"]'],
    ["dblclick", 'button[aria-label="cat, state: inactive"]'],
    ["wait", 'button[aria-label="cat, state: excluded"]']
  ]
};
