// The control seed with the first tile tagged camera, car and cat through the bulk dock, bulk
// mode off again, and "ca" typed into the search field: the suggestion list is open with the
// first suggestion active and the matched letters highlighted.
export default {
  id: "search-suggestions",
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
    ["click", ".filter-input"],
    ["fill", ".filter-input", "ca"],
    ["wait", '[role="listbox"] [role="option"]']
  ]
};
