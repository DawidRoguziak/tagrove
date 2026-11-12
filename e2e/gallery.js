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
