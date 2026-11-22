// The control seed filtered to images without the tag cat: "-cat" typed into the search field,
// then the Images segment chosen, which submits both. The item count shows the four images.
export default {
  id: "search-applied",
  theme: "dark",
  seed: "control",
  tauri: [
    ["fill", ".filter-input", "-cat"],
    ["click", '.ui-segmented-control label:has(input[value="image"])'],
    // The radio itself has opacity 0 and never counts as displayed: wait on its label.
    ["wait", '#gallery-media-kind label:has(input[value="image"]:checked)'],
    ["wait", '//*[contains(@class, "workspace-count")][normalize-space(.)="4 items"]']
  ]
};
