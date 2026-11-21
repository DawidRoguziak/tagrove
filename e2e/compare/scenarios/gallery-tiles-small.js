// The control seed in the gallery at the smallest tile size. The slider is a native range
// input: a click focuses it and Home moves it to its minimum; the click on the item count
// takes the focus, and its outline, off the slider again.
export default {
  id: "gallery-tiles-small",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", "#tile-size"],
    ["keys", "Home"],
    ["click", ".workspace-count"]
  ]
};
