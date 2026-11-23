// The control seed with the lightbox open on the first PNG (the second tile): the picture fitted
// into the stage, the action rail under it and the side panel open with the group fields, the tag
// editor and the collapsed Information panel. The steps wait for the details (the tag field is
// enabled once they are read). The last click parks the pointer on the summary of the action rail:
// the rail stays shown while the pointer rests on it, however long the capture takes, and the
// summary has no hover state. A capture shows no focus ring: the window on the private display is
// not the active one.
export default {
  id: "lightbox-image",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", 'button[data-asset-index="1"]'],
    ["wait", '[data-testid="lightbox-image"]'],
    ["wait", "#lightbox-tag-draft-input:not([disabled])"],
    ["click", ".lightbox-media-summary"]
  ]
};
