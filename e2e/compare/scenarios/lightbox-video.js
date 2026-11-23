// The control seed with the lightbox open on the first MP4 (the sixth tile). The Tauri interface
// plays it on its native surface; the GPUI interface shows a placeholder with the thumbnail as a
// poster (plan decision D8), so the comparison leaves the media stage out. The pointer is parked on
// the action rail's summary, as in lightbox-image.
export default {
  id: "lightbox-video",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[data-asset-index="5"]'],
    ["wait", '[data-lightbox-media-stage="video"]'],
    ["wait", "#lightbox-tag-draft-input:not([disabled])"],
    ["click", ".lightbox-media-summary"]
  ]
};
