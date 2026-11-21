// The control seed in the gallery at the largest tile size. The slider is a native range
// input: a click focuses it and End moves it to its maximum; the click on the item count
// takes the focus, and its outline, off the slider again.
export default {
  id: "gallery-tiles-large",
  theme: "dark",
  seed: "control",
  tauri: [
    ["click", "#tile-size"],
    ["keys", "End"],
    ["click", ".workspace-count"]
  ]
};
