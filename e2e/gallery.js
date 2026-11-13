// Select a media kind in the gallery toolbar and wait until the tiles belong to it.
// The previous result set stays rendered until the new query replies, so waiting for
// "a tile is displayed" alone is vacuous. Tile chips are English labels ("Video", "GIF").
export async function selectMediaKind(kind) {
  await $(`#gallery-media-kind label:has(input[value="${kind}"])`).click();
  await browser.waitUntil(() => browser.execute((wanted) => {
    const tiles = [...document.querySelectorAll('button[data-asset-id]')];
    const chipOf = (tile) => [...tile.querySelectorAll('span')].map(el => el.textContent).find(text => text === "Video" || text === "GIF");
    if (tiles.length === 0) return false;
    if (wanted === "all") return tiles.some(tile => chipOf(tile) === undefined);
    if (wanted === "image") return tiles.every(tile => chipOf(tile) === undefined);
    return tiles.every(tile => chipOf(tile) === (wanted === "video" ? "Video" : "GIF"));
  }, kind), { timeout: 20000, timeoutMsg: `Gallery did not show ${kind} results` });
}

// Type a query into the main search and submit it with the Search button. Escape+Enter is not
// deterministic: if the suggestion worker replies after Escape, Escape does nothing and Enter
// picks the auto-selected suggestion instead of submitting the typed text. The button submits
// the typed query directly. Callers must wait for a result that differs from the previous one.
export async function submitSearch(value) {
  const field = await $(".workspace-search-field");
  const input = await field.$(".filter-input");
  await input.click();
  await input.setValue(value);
  await browser.waitUntil(async () => (await input.getValue()) === value, {
    timeout: 10000,
    timeoutMsg: `Search input did not take ${JSON.stringify(value)}`
  });
  await field.$("button=Search").click();
}
