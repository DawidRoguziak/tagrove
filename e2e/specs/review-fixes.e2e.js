import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
let mediaRoot;
const evidence = path.resolve("artifacts/app-review/2026-09-06/fix-verification");
async function invoke(command, payload = {}) {
  const response = await browser.executeAsync(
    (name, args, done) => {
      window.__TAURI_INTERNALS__.invoke(name, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) })
      );
    },
    command,
    payload
  );
  if (response.error) throw new Error(response.error);
  return response.value;
}

async function scrollToAsset(index) {
  await browser.execute((target) => {
    const scroller = document.querySelector(".gallery-scroll");
    const tile = document.querySelector("button[data-asset-index]");
    const grid = tile.parentElement;
    const stride = tile.getBoundingClientRect().width + 12;
    const columns = Math.max(1, Math.floor((grid.clientWidth + 12) / stride));
    const margin =
      grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    scroller.scrollTop = margin + Math.floor(target / columns) * stride;
  }, index);
}


async function list() {
  return invoke("list_assets", { offset: 0, limit: 500, tagsAnd: [], tagsNot: [], kind: null, favoritesOnly: false, metaFilter: null });
}
async function submit(value) {
  await $('button[aria-label="Clear all search filters"]').click();
  const input = await $(".filter-input");
  await input.click();
  await input.setValue(value);
  assert.equal(await input.getValue(), value);
  await browser.keys("Escape");
  await browser.keys("Enter");
}
async function previewName(name) {
  await browser.waitUntil(async () => (await $('[data-lightbox-kind]').getAttribute("aria-label")).includes(name), { timeout: 15000 });
}
async function tileCount(count) {
  await browser.waitUntil(async () => (await $$("button[data-asset-id]")).length === count, { timeout: 15000 });
}

describe("review fix regressions", function () {
  this.timeout(120000);
  before(async () => {
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-review-fixes-"));
    await fs.mkdir(evidence, { recursive: true });
    await invoke("clear_library_data");
    await copyPngFixtures(mediaRoot, "review", 400);
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
  });
  after(async () => {
    await invoke("clear_library_data");
    if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(evidence, "desktop-failure.png"));
      await fs.writeFile(path.join(evidence, "desktop-failure.txt"), await browser.execute(() => `${document.body.innerText}\nInput: ${document.querySelector(".filter-input")?.value}`));
    }
  });

  it("navigates in the new group order after a save beyond the first page", async () => {
    const { items } = await list();
    assert.equal(items.length, 400);
    await invoke("set_assets_media_group_bulk", { mediaGroupKey: "review-group", updates: items.map((item, index) => ({ assetId: item.id, mediaGroupOrder: index })) });
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
    await scrollToAsset(200);
    const tile = await $('button[data-asset-index="200"]');
    await tile.waitForDisplayed({ timeout: 20000 });
    await tile.click();
    await previewName(path.basename(items[200].path));
    // Keep gallery demand on page one while the lightbox retains the distant selection.
    await scrollToAsset(0);
    const order = await $("#lightbox-media-group-order-input");
    await order.waitForDisplayed({ timeout: 10000 });
    await order.setValue("201.5");
    await $("button=Apply").click();
    await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: items[200].id })).media_group_order === 201.5);
    await browser.waitUntil(async () => !(await $('[data-navigation-status]').isExisting()) && await $("button=Apply").isEnabled());
    await $('[data-lightbox-kind]').click();
    await browser.keys("ArrowRight");
    await previewName(path.basename(items[202].path));
    await browser.keys("ArrowLeft");
    await previewName(path.basename(items[200].path));
    await browser.keys("ArrowLeft");
    await previewName(path.basename(items[201].path));
    await browser.saveScreenshot(path.join(evidence, "navigation.png"));
    await $('button[aria-label="Close preview"]').click();
  });

  it("applies literal tags from Tag list, including exclusions, and preserves typed operators", async () => {
    await invoke("clear_library_data");
    // Only disposable copies belong to this root.
    await fs.rm(mediaRoot, { recursive: true });
    await fs.mkdir(mediaRoot);
    await copyPngFixtures(mediaRoot, "literal", 7);
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    const { items } = await list();
    const names = ["tags", "tags:3", "gn:trip", "-holiday", 'say"hello', "path\\photo"];
    await invoke("set_asset_tags", { assetId: items[0].id, tags: names });
    await browser.refresh();
    await tileCount(7);
    await $('button[aria-label="Open tag list"]').click();
    await $('button[aria-label="tags, state: inactive"]').waitForDisplayed({ timeout: 10000 });
    await $('button[aria-label="tags, state: inactive"]').click();
    await $("button=Apply").click();
    await tileCount(1);
    assert.equal(await $(".filter-input").getValue(), '"tags"');
    assert.equal(Number(await $('button[data-asset-id]').getAttribute("data-asset-id")), items[0].id);
    await browser.saveScreenshot(path.join(evidence, "literal-tag.png"));
    await submit('"tags"');
    await tileCount(1);
    await $('button[aria-label="Open tag list"]').click();
    await $('button[aria-label="tags, state: inactive"]').waitForDisplayed({ timeout: 10000 });
    await $('button[aria-label="tags, state: inactive"]').doubleClick();
    await $("button=Apply").click();
    await tileCount(6);
    assert.equal(await $(".filter-input").getValue(), '-"tags"');
    for (const name of names) {
      await submit(JSON.stringify(name));
      await tileCount(1);
      await submit(`-${JSON.stringify(name)}`);
      await tileCount(6);
    }
    await submit("tags");
    await tileCount(6);
    await submit("tags:6");
    await tileCount(1);
    // Clear through the normal control before picking an autocomplete suggestion.
    await $('button[aria-label="Clear all search filters"]').click();
    await tileCount(7);
    await $(".filter-input").setValue('"tag');
    await $('[role="option"]').waitForDisplayed({ timeout: 10000 });
    await $('[role="option"]').click();
    await browser.keys("Escape");
    await browser.keys("Enter");
    await tileCount(1);
    assert.ok((await $(".filter-input").getValue()).startsWith('"tags'));
  });

  it("clears obsolete generated thumbnails through real IPC and regenerates them", async () => {
    const first = (await list()).items[0];
    await invoke("render_all_thumbnails");
    const old = (await invoke("get_asset_details", { assetId: first.id })).thumb_path;
    assert.ok(old);
    const future = new Date(Date.now() + 2000);
    await fs.utimes(first.path, future, future);
    await invoke("rescan_all_roots");
    await fs.access(old);
    // The settings view stops gallery demand while the clear command is checked.
    await $('button[aria-label="Open settings"]').click();
    const removed = await invoke("clear_all_thumbnails");
    assert.ok(removed >= 1);
    await assert.rejects(fs.access(old));
    await fs.access(first.path);
    await invoke("render_all_thumbnails");
    const regenerated = (await invoke("get_asset_details", { assetId: first.id })).thumb_path;
    assert.ok(regenerated);
    await fs.access(regenerated);
    await fs.writeFile(path.join(evidence, "thumbnail-clear.json"), JSON.stringify({ removed, old, regenerated, sourcePreserved: true }, null, 2));
  });
});
