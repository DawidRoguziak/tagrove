// The state of lightbox-image with the delete button of the action rail pressed and the English
// confirmation word typed: the inline confirmation at the bottom of the side panel with Confirm
// enabled. Nothing is deleted. The pointer is parked on the action rail's summary afterwards, as in
// lightbox-image, so the delete button shows no hover state.
export default {
  id: "lightbox-delete-confirm",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[data-asset-index="1"]'],
    ["wait", '[data-testid="lightbox-image"]'],
    ["wait", "#lightbox-tag-draft-input:not([disabled])"],
    ["click", "#lightbox-delete-button"],
    ["wait", "#lightbox-delete-confirm-input"],
    ["fill", "#lightbox-delete-confirm-input", "Yes"],
    ["wait", '//*[@data-testid="lightbox-delete-confirm-dialog"]//button[normalize-space(.)="Confirm"][not(@disabled)]'],
    ["click", ".lightbox-media-summary"]
  ]
};
