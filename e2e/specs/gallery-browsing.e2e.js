import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_GALLERY_BROWSING === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/gallery-browsing");
let root;
async function invoke(command, payload = {}) {
  const result = await browser.executeAsync(
    (name, args, done) => {
      window.__TAURI_INTERNALS__.invoke(name, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) })
      );
    },
    command,
    payload
  );
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function ready(index = 0) {
  await $(`button[data-asset-index="${index}"]`).waitForExist({ timeout: 30000 });
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const viewport = document.querySelector(".gallery-scroll").getBoundingClientRect();
        const images = [...document.querySelectorAll("button[data-asset-id] img")].filter((img) => {
          const rect = img.getBoundingClientRect();
          return rect.bottom > viewport.top && rect.top < viewport.bottom;
        });
        return images.length > 0 && images.every((img) => img.complete && img.naturalWidth > 1);
      }),
    { timeout: 30000 }
  );
}
async function scroll(index) {
  await browser.execute((index) => {
    const scroller = document.querySelector(".gallery-scroll"),
      tile = document.querySelector("button[data-asset-index]"),
      grid = document.querySelector('[data-testid="gallery-grid-content"]');
    const stride = tile.getBoundingClientRect().width + 12,
      columns = Math.max(1, Math.floor((grid.clientWidth + 12) / stride));
    const margin =
      grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    scroller.scrollTop = margin + Math.floor(index / columns) * stride;
  }, index);
  await ready(index);
}
suite("gallery browsing parity", function () {
  this.timeout(240000);
  before(async () => {
    await fs.mkdir(output, { recursive: true });
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-browsing-"));
    await copyPngFixtures(root, "browse", 2048);
    await createPlayableFixtures(root, "motion");
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    await browser.execute(() => {
      localStorage.setItem("media-tagger.language", "en");
      localStorage.setItem("media-tagger.theme", "dark");
    });
    await browser.refresh();
    await ready();
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "browsing-failure.png"));
      await fs.writeFile(
        path.join(output, "browsing-failure.txt"),
        await browser.execute(() => document.body.innerText)
      );
    }
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  it("keeps DOM bounded through 16 pages, reloads evicted ranges and resets scroll on Settings return", async () => {
    await $('#gallery-media-kind input[value="image"]').click();
    await ready();
    const samples = [];
    for (const phase of ["cold", "warm"]) {
      for (const index of [0, 256, 512, 768, 1024, 1280, 1536, 1792, 2000, 0]) {
        const started = Date.now();
        await scroll(index);
        const sample = await browser.execute(() => ({
          tiles: document.querySelectorAll("button[data-asset-id]").length,
          scroll: document.querySelector(".gallery-scroll").scrollTop,
          first: Number(document.querySelector("button[data-asset-id]").dataset.assetIndex)
        }));
        assert.ok(sample.tiles > 0 && sample.tiles < 180, "Virtualized DOM remains bounded");
        samples.push({ phase, index, elapsed: Date.now() - started, ...sample });
      }
    }
    await scroll(1792);
    await $("#open-settings-button").click();
    await $('button[aria-label="Back"]').click();
    await ready();
    assert.equal(
      await browser.execute(() => document.querySelector(".gallery-scroll").scrollTop),
      0
    );
    await $('#gallery-media-kind input[value="gif"]').click();
    await ready();
    const source = await $("button[data-asset-id] img").getAttribute("src");
    assert.ok(source.endsWith(".jpg"), "GIF tiles use generated static thumbnails");
    await fs.writeFile(path.join(output, "browsing.json"), JSON.stringify(samples, null, 2));
  });
  it("preserves gallery and search geometry in both languages and themes at three widths", async () => {
    const samples = [];
    for (const language of ["en", "pl"])
      for (const theme of ["light", "dark"])
        for (const width of [1440, 600, 320]) {
          await browser.setWindowSize(width, 900);
          await browser.execute(
            (language, theme) => {
              localStorage.setItem("media-tagger.language", language);
              localStorage.setItem("media-tagger.theme", theme);
            },
            language,
            theme
          );
          await browser.refresh();
          await ready();
          const metrics = await browser.execute(() => {
            const rect = (selector) => {
              const r = document.querySelector(selector).getBoundingClientRect();
              return { x: r.x, y: r.y, width: r.width, height: r.height };
            };
            return {
              viewport: innerWidth,
              overflow: document.documentElement.scrollWidth > innerWidth,
              header: rect(".workspace-header"),
              search: rect(".filter-input"),
              tile: rect("button[data-asset-id]"),
              theme: document.documentElement.dataset.theme,
              language: document.documentElement.lang
            };
          });
          assert.equal(metrics.overflow, false);
          assert.equal(metrics.theme, theme);
          assert.equal(metrics.language, language);
          samples.push({ language, theme, width, ...metrics });
          await browser.waitUntil(
            () =>
              browser.execute(() => {
                const viewport = document.querySelector(".gallery-scroll").getBoundingClientRect();
                return [...document.querySelectorAll("button[data-asset-id] img")].every((img) => {
                  const rect = img.getBoundingClientRect();
                  return (
                    rect.bottom <= viewport.top ||
                    rect.top >= viewport.bottom ||
                    getComputedStyle(img).opacity === "1"
                  );
                });
              }),
            { timeout: 10000 }
          );
          await browser.saveScreenshot(
            path.join(output, `gallery-${language}-${theme}-${width}.png`)
          );
        }
    await fs.writeFile(path.join(output, "geometry.json"), JSON.stringify(samples, null, 2));
  });
});
