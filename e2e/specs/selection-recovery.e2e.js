import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_SELECTION_RECOVERY === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/selection-recovery");
const pointer = actions => browser.performActions([{ id: "selection", type: "pointer", parameters: { pointerType: "mouse" }, actions }]);
async function releaseInput() {
  await browser.performActions([{ id: "modifiers", type: "key", actions: [
    { type: "keyUp", value: "\uE008" }, { type: "keyUp", value: "\uE009" }
  ] }]);
  await browser.releaseActions();
}
async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((command, payload, done) => {
    window.__TAURI_INTERNALS__.invoke(command, payload)
      .then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function count(expected) {
  await browser.waitUntil(async () => Number(await $('[data-testid="bulk-action-panel"]').getAttribute("data-selected-count")) === expected,
    { timeout: 30000, timeoutMsg: `Expected ${expected} selected` });
}
async function scrollTo(index) {
  await browser.waitUntil(() => browser.execute(index => {
    const scroller = document.querySelector(".gallery-scroll");
    const tile = document.querySelector("button[data-asset-index]");
    const grid = document.querySelector('[data-testid="gallery-grid-content"]');
    const stride = tile.getBoundingClientRect().width + 12;
    const columns = Math.max(1, Math.floor((grid.clientWidth + 12) / stride));
    const margin = grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    const before = scroller.scrollTop;
    scroller.scrollTop = index === 0 ? 0 : margin + Math.floor(index / columns) * stride;
    const target = document.querySelector(`button[data-asset-index="${index}"]`);
    window.selectionScrolls = [...(window.selectionScrolls ?? []).slice(-39), {
      index, before, after: scroller.scrollTop, margin, stride, columns,
      first: tile.dataset.assetIndex, target: target?.getBoundingClientRect().toJSON(),
      viewport: scroller.getBoundingClientRect().toJSON()
    }];
    if (!target) return false;
    const rect = target.getBoundingClientRect();
    const viewport = scroller.getBoundingClientRect();
    // WebKit rounds scrollTop while held-drag scrolling retains fractional offsets.
    return rect.top >= viewport.top - 1 && rect.bottom <= viewport.bottom + 1;
  }, index), { timeout: 30000, timeoutMsg: `Scroll did not reach ${index}` });
}
suite("selection recovery after banner layout", function () {
  this.timeout(180000);
  let root;
  before(async () => {
    assert.ok(process.env.XDG_DATA_HOME?.startsWith("/tmp/mediatagger-"));
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-selection-recovery-"));
    await fs.mkdir(output, { recursive: true });
    await copyPngFixtures(root, "selection", 2052);
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    await browser.setWindowSize(1200, 850);
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
      await fs.writeFile(path.join(output, "failure.json"), JSON.stringify(await browser.execute(() => ({
        text: document.body.innerText, reads: window.selectionReads, geometry: window.selectionGeometry,
        scrolls: window.selectionScrolls
      })), null, 2));
    }
    await releaseInput();
    await browser.execute(() => window.restoreSelectionProbe?.());
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  for (const kind of ["shift", "rectangle"]) {
    it(`recovers the original additive ${kind} range after its ID range read fails`, async () => {
      await releaseInput();
      await browser.refresh();
      await $('button[data-asset-index="0"]').waitForDisplayed();
      await $('button[aria-label="Enable bulk actions"]').click();
      await scrollTo(2051);
      await $('button[data-asset-index="2051"]').click();
      await count(1);
      await scrollTo(0);
      await $('button[data-asset-index="0"]').click();
      await count(2);
      const endpoint = await browser.execute(() => {
        const tile = document.querySelector('button[data-asset-index="0"]');
        const grid = document.querySelector('[data-testid="gallery-grid-content"]');
        const columns = Math.max(1, Math.floor((grid.clientWidth + 12) / (tile.getBoundingClientRect().width + 12)));
        return (Math.floor(1792 / columns) + 1) * columns - 1;
      });
      if (kind === "rectangle") {
        const start = await browser.execute(() => {
          const rect = document.querySelector('button[data-asset-index="0"]').getBoundingClientRect();
          return { x: Math.ceil(rect.left + 1), y: Math.ceil(rect.top + 1) };
        });
        await browser.performActions([{ id: "modifiers", type: "key", actions: [{ type: "keyDown", value: "\uE009" }] }]);
        await pointer([{ type: "pointerMove", duration: 0, ...start }, { type: "pointerDown", button: 0 },
          // Hold below the 48px top edge zone: edge auto-scroll would otherwise keep the
          // fast-scroll pause alive while this test jumps the scroller, so tiles never mount.
          { type: "pause", duration: 200 }, { type: "pointerMove", duration: 100, x: start.x + 10, y: start.y + 120 }]);
        await $('[data-testid="gallery-selection-rectangle"]').waitForDisplayed();
      }
      // Traverse many pages: the range spans unloaded and evicted pages, which the
      // binary ID range read covers without loading them.
      for (let index = 128; index <= 1792; index += 128) await scrollTo(index);
      await scrollTo(endpoint);
      await browser.execute(() => {
        const original = window.fetch;
        window.selectionReads = [];
        window.selectionGeometry = [];
        window.failSelectionPage = true;
        const gallery = document.querySelector('[data-testid="gallery-grid"]');
        const grid = document.querySelector('[data-testid="gallery-grid-content"]');
        const scroller = document.querySelector('.gallery-scroll');
        const measure = () => window.selectionGeometry.push({
          banner: Boolean(document.querySelector('[data-testid="gallery-load-error"]')),
          margin: grid.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop
        });
        const observer = new ResizeObserver(measure);
        observer.observe(gallery);
        measure();
        window.restoreSelectionProbe = () => { window.fetch = original; observer.disconnect(); };
        window.fetch = async (resource, options) => {
          if (decodeURIComponent(String(resource)).includes("get_asset_query_id_range")) {
            const payload = JSON.parse(options.body);
            const failed = window.failSelectionPage;
            window.selectionReads.push({ start: payload.start, end: payload.end, failed });
            // A negative unsigned start fails argument deserialization in the real command.
            if (failed) return original(resource, { ...options, body: JSON.stringify({ ...payload, start: -1 }) });
          }
          return original(resource, options);
        };
      });
      if (kind === "shift") {
        await browser.performActions([{ id: "modifiers", type: "key", actions: [
          { type: "keyDown", value: "\uE009" }, { type: "keyDown", value: "\uE008" }
        ] }]);
        const target = await browser.execute(index => {
          const rect = document.querySelector(`button[data-asset-index="${index}"]`).getBoundingClientRect();
          return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
        }, endpoint);
        await pointer([{ type: "pointerMove", duration: 0, ...target }, { type: "pointerDown", button: 0 }, { type: "pointerUp", button: 0 }]);
      } else {
        const end = await browser.execute(index => {
          const rect = document.querySelector(`button[data-asset-index="${index}"]`).getBoundingClientRect();
          return { x: Math.floor(rect.right - 1), y: Math.floor(rect.bottom - 1) };
        }, endpoint);
        await pointer([{ type: "pointerMove", duration: 100, ...end }, { type: "pointerUp", button: 0 }]);
      }
      await releaseInput();
      await browser.waitUntil(() => browser.execute(() => window.selectionReads.some(read => read.failed)));
      await browser.waitUntil(() => browser.execute(() => {
        const first = window.selectionGeometry[0];
        return window.selectionGeometry.some(sample => sample.banner && sample.margin > first.margin);
      }));
      await count(2);
      // Bring the banner into view.
      await scrollTo(0);
      const retry = await $('[data-testid="gallery-load-error"] button');
      await retry.waitForDisplayed();
      const before = await browser.execute(() => window.selectionReads.length);
      await browser.saveScreenshot(path.join(output, `${kind}-failed.png`));
      await browser.execute(() => { window.failSelectionPage = false; });
      await retry.click();
      await count(endpoint + 2);
      assert.equal(await $('[data-testid="gallery-load-error"]').isExisting(), false);
      assert.equal(await $('button[data-asset-index="0"]').getAttribute("aria-pressed"), "true");
      const evidence = await browser.execute(() => ({ reads: window.selectionReads, geometry: window.selectionGeometry }));
      // Retry repeats the original range in one successful binary read.
      const retried = evidence.reads.slice(before);
      assert.equal(retried.length, 1);
      assert.deepEqual(retried[0], { start: 0, end: endpoint, failed: false });
      await scrollTo(2051);
      assert.equal(await $('button[data-asset-index="2051"]').getAttribute("aria-pressed"), "true");
      await fs.writeFile(path.join(output, `${kind}.json`), JSON.stringify({ endpoint, before, ...evidence }, null, 2));
      await browser.saveScreenshot(path.join(output, `${kind}-recovered.png`));
    });
  }
});
