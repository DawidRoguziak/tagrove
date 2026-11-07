import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_LIGHTBOX_RECOVERY === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/lightbox-recovery");
const imageSelector = '[data-testid="lightbox-image"]';
async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((command, payload, done) => {
    window.__TAURI_INTERNALS__.invoke(command, payload)
      .then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function imageNamed(name) {
  await browser.waitUntil(() => browser.execute((selector, name) => {
    const image = document.querySelector(selector);
    return image?.alt === name && image.complete && image.naturalWidth > 0;
  }, imageSelector, name));
}
suite("lightbox recovery and image ownership", function () {
  this.timeout(120000);
  let root, rows;
  before(async () => {
    assert.ok(process.env.XDG_DATA_HOME?.startsWith("/tmp/mediatagger-"));
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-lightbox-recovery-"));
    await fs.mkdir(output, { recursive: true });
    await copyPngFixtures(root, "recovery", 132);
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    rows = (await invoke("list_assets", { offset: 0, limit: 132, tagsAnd: [], tagsNot: [], kind: null, favoritesOnly: false })).items;
    assert.equal(rows.length, 132);
  });
  after(async () => {
    await browser.execute(() => window.restoreLightboxFetch?.());
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") await browser.saveScreenshot(path.join(output, "failure.png"));
  });
  it("retries a failed page through the visible lightbox action and preserves Previous", async () => {
    await browser.setWindowSize(1000, 800);
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed();
    await browser.execute(() => {
      const original = window.fetch;
      window.lightboxReads = [];
      window.failLightboxPage = true;
      window.restoreLightboxFetch = () => { window.fetch = original; };
      window.fetch = async (resource, options) => {
        if (decodeURIComponent(String(resource)).includes("get_asset_query_page")) {
          const payload = JSON.parse(options.body);
          window.lightboxReads.push({ offset: payload.offset, failed: window.failLightboxPage });
          if (window.failLightboxPage && payload.offset === 128) {
            // A negative unsigned offset provokes a real native argument rejection.
            return original(resource, { ...options, body: JSON.stringify({ ...payload, offset: -1 }) });
          }
        }
        return original(resource, options);
      };
      const scroller = document.querySelector(".gallery-scroll");
      const tile = document.querySelector("button[data-asset-index]");
      const grid = document.querySelector('[data-testid="gallery-grid-content"]');
      const stride = tile.getBoundingClientRect().width + 12;
      const columns = Math.max(1, Math.floor((grid.clientWidth + 12) / stride));
      const margin = grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
      scroller.scrollTop = margin + Math.floor(127 / columns) * stride;
    });
    await $('button[data-asset-index="127"]').waitForDisplayed();
    await $('button[data-asset-index="127"]').click();
    await imageNamed(path.basename(rows[127].path));
    await browser.keys("ArrowRight");
    const retry = await $('[data-navigation-status="failed"] button');
    await retry.waitForDisplayed();
    await browser.keys("ArrowLeft");
    await imageNamed(path.basename(rows[126].path));
    await browser.keys("ArrowRight");
    await imageNamed(path.basename(rows[127].path));
    await browser.keys("ArrowRight");
    await retry.waitForDisplayed();
    const before = await browser.execute(() => window.lightboxReads.length);
    await browser.saveScreenshot(path.join(output, "page-failed.png"));
    await browser.execute(() => { window.failLightboxPage = false; });
    await retry.click();
    await imageNamed(path.basename(rows[128].path));
    const reads = await browser.execute(() => window.lightboxReads);
    assert.equal(reads.length, before + 1);
    assert.deepEqual(reads.at(-1), { offset: 128, failed: false });
    assert.equal(await $('[data-navigation-status="failed"]').isExisting(), false);
    await fs.writeFile(path.join(output, "page-reads.json"), JSON.stringify(reads, null, 2));
    await browser.saveScreenshot(path.join(output, "page-recovered.png"));
    await browser.execute(() => window.restoreLightboxFetch());
  });
  it("ignores an actual detached-image load after the next image has loaded", async () => {
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed();
    await $('button[data-asset-index="0"]').click();
    await imageNamed(path.basename(rows[0].path));
    await browser.execute(selector => { window.previousLightboxImage = document.querySelector(selector); }, imageSelector);
    await browser.keys("ArrowRight");
    await imageNamed(path.basename(rows[1].path));
    await browser.keys("+");
    await browser.waitUntil(() => browser.execute(selector =>
      document.querySelector(selector).style.getPropertyValue("--lightbox-image-zoom") === "1.25", imageSelector));
    const before = await browser.execute(selector => document.querySelector(selector).style.cssText, imageSelector);
    // Releasing a new synthetic source on the retained detached node produces a real
    // WebKit load event. No application handler or state setter is called by the test.
    const late = await browser.executeAsync(done => {
      const image = window.previousLightboxImage;
      image.addEventListener("load", () => requestAnimationFrame(() => requestAnimationFrame(() =>
        done({ connected: image.isConnected, width: image.naturalWidth, height: image.naturalHeight }))), { once: true });
      image.src = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="2000"><rect width="40" height="2000" fill="red"/></svg>')}`;
    });
    assert.deepEqual(late, { connected: false, width: 40, height: 2000 });
    assert.equal(await browser.execute(selector => document.querySelector(selector).style.cssText, imageSelector), before);
    await browser.keys("+");
    await browser.waitUntil(() => browser.execute(selector =>
      document.querySelector(selector).style.getPropertyValue("--lightbox-image-zoom") === "1.5625", imageSelector));
    await browser.keys("0");
    await browser.waitUntil(() => browser.execute(selector =>
      document.querySelector(selector).style.getPropertyValue("--lightbox-image-zoom") === "1", imageSelector));
    await fs.writeFile(path.join(output, "late-image.json"), JSON.stringify({ late, before }, null, 2));
    await browser.saveScreenshot(path.join(output, "late-image.png"));
  });
});
