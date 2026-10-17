import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const suite = process.env.MEDIATAGGER_THUMBNAIL_FADE === "1" ? describe : describe.skip;
const reduced = process.env.MEDIATAGGER_REDUCED_MOTION === "1";
const output = path.resolve("artifacts/thumbnail-fade", new Date().toISOString().replace(/[:.]/g, "-"));
let root;

async function invoke(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (response.error) throw new Error(response.error);
  return response.value;
}

async function startSampling() {
  await browser.execute(() => {
    const probe = { frames: [], running: true };
    window.__thumbnailProbe = probe;
    const ids = new WeakMap();
    let nextId = 0;
    const sample = () => {
      const images = [...document.querySelectorAll("button[data-asset-id] img")].map(image => {
        if (!ids.has(image)) ids.set(image, ++nextId);
        const style = getComputedStyle(image);
        const rect = image.getBoundingClientRect();
        return { id: ids.get(image), opacity: Number(style.opacity), ready: image.complete && image.naturalWidth > 1,
          animation: style.animationName,
          visible: rect.bottom > 100 && rect.top < innerHeight, tileOpacity: getComputedStyle(image.parentElement).opacity };
      });
      probe.frames.push({ images, galleryOpacity: getComputedStyle(document.querySelector(".gallery-scroll")).opacity });
      if (probe.running) requestAnimationFrame(sample);
    };
    sample();
  });
}

async function finishSampling(name) {
  await browser.pause(450);
  const frames = await browser.execute(() => {
    window.__thumbnailProbe.running = false;
    return window.__thumbnailProbe.frames;
  });
  await fs.writeFile(path.join(output, `${name}.json`), JSON.stringify(frames));
  assert.ok(frames.length > 3, "Sample real rendering frames");
  let loadedVisible = 0;
  for (const frame of frames) {
    assert.equal(frame.galleryOpacity, "1", "Gallery container stays opaque");
    for (const image of frame.images) {
      assert.equal(image.tileOpacity, "1", "Tile background and controls stay opaque");
      assert.equal(image.opacity, 1, "Gallery images never fade in, including newly mounted rows");
      assert.equal(image.animation, "none", "Gallery images have no entrance animation");
      if (image.ready && image.visible) loadedVisible++;
    }
  }
  assert.ok(loadedVisible > 0, "Loaded thumbnails appear in the viewport");
}

suite("immediate gallery thumbnails", function () {
  this.timeout(120000);
  before(async () => {
    await fs.mkdir(output, { recursive: true });
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-thumbnail-fade-"));
    const colors = ["#f25f5c", "#ffe066", "#247ba0", "#70c1b3", "#b388eb", "#ff9f1c"];
    for (const [index, color] of colors.entries()) {
      execFileSync("convert", ["-size", "240x240", `xc:${color}`, "-fill", "white", "-draw", "circle 120,120 120,60",
        path.join(root, `color-${index}.png`)]);
    }
    for (let index = colors.length; index < 192; index++) {
      await fs.copyFile(path.join(root, `color-${index % colors.length}.png`), path.join(root, `color-${index}.png`));
    }
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await invoke("list_assets", { offset: 0, limit: 256, tagsAnd: [], tagsNot: [], kind: "all", favoritesOnly: false });
    await invoke("merge_asset_tags_bulk", { assetIds: page.items.filter((_, index) => index % 2 === 0).map(item => item.id), tags: ["fade"] });
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 30000 });
    assert.equal(await browser.execute(() => matchMedia("(prefers-reduced-motion: reduce)").matches), reduced);
  });
  afterEach(async function () {
    await browser.releaseActions();
    await browser.execute(() => { if (window.__thumbnailProbe) window.__thumbnailProbe.running = false; });
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
    }
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  for (const theme of ["dark", "light"]) {
    it(`shows new images immediately during rapid filtering and scrolling in ${theme}, reduced motion ${reduced}`, async () => {
      await $('button[aria-label="Open settings"]').click();
      await $(`.theme-choice:has(input[value="${theme}"])`).click();
      await $('button[aria-label="Back"]').click();
      await $('button[data-asset-index="0"]').waitForDisplayed();
      await browser.pause(450);
      await startSampling();
      for (const query of ["fade", "-fade", "fade"]) {
        await $(".filter-input").setValue(query);
        await browser.keys("Enter");
      }
      await $('button[aria-label="Clear all search filters"]').click();
      await $('button[data-asset-index="0"]').waitForDisplayed();
      await finishSampling(`${theme}-filters-${reduced}`);
      await browser.execute(() => { document.querySelector(".gallery-scroll").scrollTop = 0; });
      await browser.pause(450);
      await startSampling();
      await browser.performActions([{ type: "pointer", id: "gallery-pointer", parameters: { pointerType: "mouse" }, actions: [
        { type: "pointerMove", x: 500, y: 400, duration: 0 }
      ] }]);
      await browser.performActions([{ type: "wheel", id: "gallery-scroll", actions: [
        { type: "scroll", x: 500, y: 400, deltaX: 0, deltaY: 1400, duration: 500 },
        { type: "scroll", x: 500, y: 400, deltaX: 0, deltaY: 1400, duration: 500 }
      ] }]);
      assert.ok(await browser.execute(() => document.querySelector(".gallery-scroll").scrollTop) > 0, "Native wheel scrolls the gallery");
      await finishSampling(`${theme}-scroll-${reduced}`);
      await browser.saveScreenshot(path.join(output, `${theme}-${reduced}.png`));
    });
  }
});
