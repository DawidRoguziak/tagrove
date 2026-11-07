import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
import { listAssetsForAssertions } from "../assets.js";

const suite = process.env.MEDIATAGGER_LIGHTBOX_IMAGES === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/lightbox-images");
let root;
let previewAssetId;
async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((command, payload, done) => {
    window.__TAURI_INTERNALS__.invoke(command, payload).then(value => done({value}), error => done({error: String(error)}));
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function settledSidebar() {
  await browser.waitUntil(() => browser.execute(() => {
    const sidebar = document.getElementById("lightbox-sidebar");
    return ["none", "0px", "0px 0px"].includes(getComputedStyle(sidebar).translate)
      && sidebar.getAnimations().every(animation => animation.playState !== "running");
  }));
}
suite("image lightbox parity", function () {
  this.timeout(120000);
  before(async () => {
    assert.ok(process.env.XDG_DATA_HOME?.startsWith("/tmp/mediatagger-"));
    await fs.mkdir(output, {recursive: true});
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-lightbox-images-"));
    await copyPngFixtures(root, "synthetic", 1);
    await fs.copyFile(path.join(root,"synthetic-1.png"),path.join(root,"synthetic-2.png"));
    await invoke("clear_library_data");
    await invoke("add_scan_root", {path: root});
    await invoke("rescan_all_roots");
    const page = await listAssetsForAssertions({offset:0, limit:2, tagsAnd:[], tagsNot:[], kind:"image", favoritesOnly:false});
    previewAssetId = page.items.find(item => path.basename(item.path) === "synthetic-1.png").id;
    for (const item of page.items) await invoke("set_asset_tags", {assetId:item.id,tags:item.id === previewAssetId ? ["synthetic-tag", "long_".repeat(25)] : ["suggestion-only"]});
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root,{recursive:true,force:true});
  });
  it("preserves geometry in both themes and languages, including a narrow drawer", async () => {
    const samples = [];
    for (const language of ["en","pl"]) for (const theme of ["light","dark"]) for (const width of [320,600,1000]) {
      await browser.execute((language,theme) => {localStorage.setItem("media-tagger.language",language);localStorage.setItem("media-tagger.theme",theme);},language,theme);
      await browser.setWindowSize(width,720);
      await browser.refresh();
      const tile = await $(`button[data-asset-id="${previewAssetId}"]`);
      await tile.waitForDisplayed(); await tile.click();
      await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-testid="lightbox-image"]')?.naturalWidth > 0));
      if (width < 768) await $('#lightbox-sidebar-trigger-button').click();
      await settledSidebar();
      await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-testid="lightbox-tag-list"]')?.textContent.includes("synthetic-tag")));
      // WebKit can retain the drawer's outgoing scroll extent for one layout after
      // its translate transition finishes. Measure after that extent settles too.
      await browser.waitUntil(() => browser.execute(() => {
        const layout = document.querySelector(".lightbox-layout");
        return layout.scrollWidth === layout.clientWidth;
      }));
      const sample = await browser.execute(() => {
        const selectors = ["[data-lightbox-kind]", ".lightbox-layout", "[data-lightbox-media-stage]", '[data-testid="lightbox-image"]', "#lightbox-sidebar", '[data-testid="lightbox-tag-list"]', '.lightbox-action-island'];
        const rects = Object.fromEntries(selectors.map(selector => {
          const el = document.querySelector(selector), r = el.getBoundingClientRect();
          return [selector,{x:r.x,y:r.y,width:r.width,height:r.height,clientWidth:el.clientWidth,scrollWidth:el.scrollWidth}];
        }));
        return {rects,overflow:document.documentElement.scrollWidth > innerWidth};
      });
      assert.equal(sample.overflow,false);
      samples.push({language,theme,width,...sample});
      await browser.saveScreenshot(path.join(output,`${language}-${theme}-${width}.png`));
      await $('#lightbox-sidebar header button:last-child').click();
      await $('[data-lightbox-kind]').waitForExist({reverse:true});
    }
    await fs.writeFile(path.join(output,"geometry.json"),JSON.stringify(samples,null,2));
  });
  it("keeps suggestion Escape inside the lightbox and returns focus to the gallery", async () => {
    await browser.execute(() => localStorage.setItem("media-tagger.language","en"));
    await browser.setWindowSize(1000,720); await browser.refresh();
    const tile = await $(`button[data-asset-id="${previewAssetId}"]`); await tile.waitForDisplayed(); await tile.click();
    await $('#lightbox-tag-draft-input').waitForEnabled();
    await $('#lightbox-tag-draft-input').setValue("sug");
    await $('[role="option"]').waitForDisplayed();
    await browser.keys("Escape");
    assert.equal(await $('[data-lightbox-kind]').isExisting(),true);
    assert.equal(await $('#lightbox-tag-draft-input').getValue(),"sug");
    await $('#lightbox-sidebar header button:last-child').click();
    await $('[data-lightbox-kind]').waitForExist({reverse:true});
    await browser.waitUntil(() => browser.execute(id => document.activeElement?.matches(`button[data-asset-id="${id}"]`), previewAssetId));
  });
});
