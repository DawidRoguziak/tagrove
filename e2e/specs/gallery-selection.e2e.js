import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_GALLERY_SELECTION === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/gallery-selection");
let mediaRoot;
const pointer = actions => browser.performActions([{ id: "rectangle", type: "pointer", parameters: { pointerType: "mouse" }, actions }]);
async function invoke(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (response.error) throw new Error(response.error);
  return response.value;
}
async function geometry() {
  return browser.execute(() => {
    const tile = document.querySelector("button[data-asset-index]");
    const grid = tile.parentElement.getBoundingClientRect();
    const rect = tile.getBoundingClientRect();
    const viewport = document.querySelector(".gallery-scroll").getBoundingClientRect();
    return { left: Math.round(grid.left), top: Math.round(grid.top), right: Math.floor(grid.right),
      size: rect.width, bottom: Math.floor(viewport.bottom), viewportTop: Math.ceil(viewport.top) };
  });
}
async function count(expected) {
  await browser.waitUntil(async () => (await $('[data-testid="bulk-header-panel"]').getText()).includes(`Selected: ${expected}`), { timeout: 30000, timeoutMsg: `Expected ${expected} selected` });
}
async function clear() {
  await browser.keys("Escape");
  await count(0);
}
async function dragStart(x, y, endX, endY) {
  await pointer([{ type: "pointerMove", duration: 0, x, y }, { type: "pointerDown", button: 0 },
    { type: "pause", duration: 200 },
    { type: "pointerMove", duration: 250, x: endX, y: endY }]);
  await $('[data-testid="gallery-selection-rectangle"]').waitForDisplayed();
}
async function release() {
  await pointer([{ type: "pointerUp", button: 0 }]);
  await browser.releaseActions();
}
suite("gallery additive clicks and rectangle selection", function () {
  this.timeout(180000);
  before(async () => {
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-selection-"));
    await fs.mkdir(output, { recursive: true });
    await copyPngFixtures(mediaRoot, "selection", 2048);
    await createPlayableFixtures(mediaRoot, "mixed");
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 30000 });
  });
  beforeEach(async () => {
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 30000 });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
      await fs.writeFile(path.join(output, "failure.json"), JSON.stringify(await browser.execute(() => ({ text: document.body.innerText })), null, 2));
    }
    await browser.releaseActions();
    await browser.keys("Escape");
  });
  after(async () => {
    await browser.releaseActions();
    await invoke("clear_library_data");
    if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
  });
  it("ignores accidental quick drags and keeps ordinary clicks toggling", async () => {
    await $('button[aria-label="Enable bulk actions"]').click();
    await $('button[data-asset-index="1"]').click();
    await count(1);
    const g = await geometry();
    await pointer([
      { type: "pointerMove", duration: 0, x: g.left - 5, y: g.top + 20 },
      { type: "pointerDown", button: 0 },
      { type: "pointerMove", duration: 0, x: g.left + 25, y: g.top + 20 },
      { type: "pointerUp", button: 0 }
    ]);
    await browser.releaseActions();
    await count(1);
    assert.equal(await $('button[data-asset-index="1"]').getAttribute("aria-pressed"), "true");
    assert.equal(await $('button[data-asset-index="0"]').getAttribute("aria-pressed"), "false");
    assert.equal(await $('[data-testid="gallery-selection-rectangle"]').isExisting(), false);
    await $('button[data-asset-index="0"]').click(); await count(2);
    await $('button[data-asset-index="0"]').click(); await count(1);
  });
  for (const omitTrailingClick of [false, true]) {
    it(`opens an image on the first click after leaving rectangle selection, omitted trailing click: ${omitTrailingClick}`, async () => {
      await $('button[aria-label="Enable bulk actions"]').click();
      const g = await geometry();
      await dragStart(g.left + 5, g.top + 5, Math.round(g.left + g.size * 1.5), Math.round(g.top + g.size * 1.5));
      if (omitTrailingClick) {
        // Model a drag-ending click that never reaches React, using real pointer input.
        await browser.execute(() => {
          window.addEventListener("click", event => {
            event.preventDefault();
            event.stopImmediatePropagation();
          }, { capture: true, once: true });
        });
      }
      await release();
      await count(4);
      await $('button[aria-label="Disable bulk actions"]').click();
      await $('button[data-asset-id] img[alt$=".png"]').click();
      await $('[data-lightbox-kind="image"]').waitForDisplayed();
      await browser.saveScreenshot(path.join(output, `first-click-${omitTrailingClick}.png`));
      await $('button[aria-label="Close preview"]').click();
    });
  }
  for (const theme of ["dark", "light"]) {
    it(`uses additive clicks, replacing rectangles and modifiers in ${theme}`, async () => {
      await $('button[aria-label="Open settings"]').click();
      await $(`.theme-choice:has(input[value="${theme}"])`).click();
      await $('button[aria-label="Back"]').click();
      await $('button[aria-label="Enable bulk actions"]').click();
      await $('button[data-asset-index="0"]').waitForDisplayed();
      await $('button[data-asset-index="0"]').click();
      await $('button[data-asset-index="1"]').click();
      await $('button[data-asset-index="0"]').click();
      await count(1);
      assert.equal(await $('button[data-asset-index="0"]').getAttribute("aria-pressed"), "false");
      assert.equal(await $('button[data-asset-index="1"]').getAttribute("aria-pressed"), "true");
      await clear();
      const g = await geometry();
      await dragStart(g.left + 5, g.top + 5, Math.round(g.left + g.size + 15), Math.round(g.top + g.size + 15));
      assert.equal(await $$('button[data-asset-index][aria-pressed="true"]').length, 4);
      const overlay = await browser.execute(() => {
        const rect = document.querySelector('[data-testid="gallery-selection-rectangle"]').getBoundingClientRect();
        const style = getComputedStyle(document.querySelector('[data-testid="gallery-selection-rectangle"]'));
        return { left: rect.left, top: rect.top, background: style.backgroundColor, border: style.borderTopWidth };
      });
      assert.ok(Math.abs(overlay.left - (g.left + 5)) <= 1);
      assert.ok(Math.abs(overlay.top - (g.top + 5)) <= 1);
      assert.notEqual(overlay.background, "rgba(0, 0, 0, 0)");
      assert.equal(overlay.border, "1px");
      await browser.saveScreenshot(path.join(output, `${theme}-rectangle.png`));
      await pointer([{ type: "pointerMove", duration: 150, x: g.left + 25, y: g.top + 25 }]);
      await browser.waitUntil(async () => (await $$('button[data-asset-index][aria-pressed="true"]')).length === 1);
      await release(); await count(1);
      await $('button[data-asset-index="0"]').click(); await count(0);
      await $('button[data-asset-index="0"]').click(); await count(1);
      // Native Ctrl-click toggles, then Ctrl-drag retains the existing ID.
      await browser.performActions([{ id: "modifier", type: "key", actions: [{ type: "keyDown", value: "\uE009" }] }]);
      await $('button[data-asset-index="0"]').click(); await count(0);
      await browser.releaseActions();
      await $('button[data-asset-index="2"]').click();
      await browser.performActions([{ id: "modifier", type: "key", actions: [{ type: "keyDown", value: "\uE009" }] }]);
      await dragStart(g.left - 5, g.top + 5, g.left + 25, g.top + 25);
      await release(); await count(2);
      // A rectangle without modifiers replaces both IDs.
      await dragStart(g.left + 25, g.top + 25, g.left + 5, g.top + 5);
      await release(); await count(1);
      await browser.keys("Escape"); await count(0);
      await $('button[data-asset-index="0"]').click(); await count(1);
      await dragStart(g.left + 5, g.top + 5, g.left + 35, g.top + 35);
      await browser.keys("Escape");
      await browser.waitUntil(() => browser.execute(() => !document.querySelector('[data-testid="gallery-selection-rectangle"]')));
      await release(); await count(0);
      await $('button[aria-label="Disable bulk actions"]').click();
      await $('button[data-asset-id] img[alt$=".png"]').click();
      await browser.waitUntil(() => browser.execute(() => Boolean(document.querySelector('[data-lightbox-kind="image"]'))));
      await $('button[aria-label="Close preview"]').click();
      await browser.waitUntil(() => browser.execute(() => !document.querySelector('[data-lightbox-kind]')));
    });
  }
  it("autoscrolls and resolves all 2051 IDs across unloaded and evicted pages", async () => {
    await $('button[aria-label="Enable bulk actions"]').click();
    const g = await geometry();
    const startScroll = await browser.execute(() => document.querySelector(".gallery-scroll").scrollTop);
    await dragStart(g.left + 1, g.top + 1, g.right - 1, g.bottom - 4);
    await browser.waitUntil(async () => (await browser.execute(() => document.querySelector(".gallery-scroll").scrollTop)) > startScroll + 50);
    // Scroll while holding a real captured pointer. Intermediate pages stay unloaded.
    await browser.execute(() => { const scroller = document.querySelector(".gallery-scroll"); scroller.scrollTop = scroller.scrollHeight; });
    await $('button[data-asset-index="2050"]').waitForDisplayed({ timeout: 30000 });
    const end = await browser.execute(() => {
      const rect = document.querySelector('button[data-asset-index="2050"]').getBoundingClientRect();
      return Math.floor(rect.bottom - 1);
    });
    await pointer([{ type: "pointerMove", duration: 100, x: g.right - 1, y: end }]);
    await release(); await count(2051);
    assert.ok((await $$('button[data-asset-index]')).length < 200);
    await browser.execute(() => { document.querySelector(".gallery-scroll").scrollTop = 0; });
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 30000 });
    assert.equal(await $('button[data-asset-index="0"]').getAttribute("aria-pressed"), "true");
    await browser.saveScreenshot(path.join(output, "complete-selection.png"));
    await clear();
    await $('button[aria-label="Disable bulk actions"]').click();
  });
  it("preserves selection on empty-space clicks and starts rectangles below a short gallery", async () => {
    await $('#gallery-media-kind label:has(input[value="gif"])').click();
    await $('button=Search').click();
    await browser.waitUntil(() => browser.execute(() => document.querySelectorAll('button[data-asset-index]').length === 1));
    await $('button[aria-label="Enable bulk actions"]').click();
    await $('button[data-asset-index="0"]').click(); await count(1);
    await $('#bulk-group-key-input').click(); await count(1);
    await $('.filter-input').click(); await count(1);
    const g = await geometry();
    await pointer([{ type: "pointerMove", duration: 0, x: g.left + 20, y: g.bottom - 25 },
      { type: "pointerDown", button: 0 }, { type: "pointerUp", button: 0 }]);
    await count(1);
    await clear();
    await dragStart(g.left + 20, g.bottom - 80, g.left + 40, g.top + 20);
    await release(); await count(1);
    await browser.saveScreenshot(path.join(output, "short-gallery.png"));
    await $('button[aria-label="Disable bulk actions"]').click();
  });

});
