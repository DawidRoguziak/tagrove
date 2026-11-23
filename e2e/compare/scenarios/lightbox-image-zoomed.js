// The state of lightbox-image after four presses of "+" (the zoom factor is 1.25 to the fourth,
// around the centre of the stage) and a drag of the zoomed picture by 120 pixels to the right and
// 60 down, which stays inside the pan's bounds. The pointer is parked on the action rail's summary
// afterwards, as in lightbox-image.
export default {
  id: "lightbox-image-zoomed",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[data-asset-index="1"]'],
    ["wait", '[data-testid="lightbox-image"]'],
    ["wait", "#lightbox-tag-draft-input:not([disabled])"],
    ["keys", "+"],
    ["keys", "+"],
    ["keys", "+"],
    ["keys", "+"],
    ["drag", '[data-testid="lightbox-image"]', "+120,+60"],
    ["click", ".lightbox-media-summary"]
  ]
};
