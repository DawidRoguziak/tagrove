// The control seed with the lightbox open on the GIF (the first tile). The Tauri interface plays
// the animation; the GPUI interface shows its first frame with a GIF chip (plan decision D8), so
// the comparison leaves the media stage out. The pointer is parked on the action rail's summary,
// as in lightbox-image.
export default {
  id: "lightbox-gif",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[data-asset-index="0"]'],
    ["wait", '[data-testid="lightbox-image"]'],
    ["wait", "#lightbox-tag-draft-input:not([disabled])"],
    ["click", ".lightbox-media-summary"]
  ]
};
