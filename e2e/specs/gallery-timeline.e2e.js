import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

// Stages 08 and 09 of docs/plans/grid-performance: the month timeline and a select-all
// bulk write through a selection spec. Synthetic PNGs only, in a fresh temporary root.
const suite = process.env.MEDIATAGGER_GALLERY_TIMELINE === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/gallery-timeline");
const PER_MONTH = 1000;
const MONTHS = 12; // December 2023 (newest) back to January 2023
const OLDEST = 5; // June 2012, the last section
const TOTAL = PER_MONTH * MONTHS + OLDEST;
const TAG = "timeline-proof";
let mediaRoot;
const timings = {};

async function invoke(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (response.error) throw new Error(response.error);
  return response.value;
}
const slider = () => $('[role="slider"][aria-label="Timeline"]');
async function valueText(expected) {
  await browser.waitUntil(async () => (await slider().getAttribute("aria-valuetext")) === expected, {
    timeout: 15000,
    timeoutMsg: `Expected timeline month ${expected}, got ${await slider().getAttribute("aria-valuetext")}`
  });
}
// WebKitWebDriver rejects `.click()` on the rail (its sticky container is
// `pointer-events: none`), although native pointer actions reach it; keyboard cases focus it.
async function focusSlider() {
  await browser.execute(() => document.querySelector('[role="slider"][aria-label="Timeline"]').focus());
}
/** Milliseconds from a timeline key press until the target tile shows loaded data. */
async function jump(key, index) {
  await focusSlider();
  const started = Date.now();
  await browser.keys(key);
  await $(`button[data-asset-index="${index}"]`).waitForExist({ timeout: 15000 });
  return Date.now() - started;
}
async function count(expected) {
  await browser.waitUntil(async () => (await $('[data-testid="bulk-action-panel"]').getAttribute("data-selected-count")) === String(expected), {
    timeout: 30000,
    timeoutMsg: `Expected ${expected} selected`
  });
}
async function utcMonth(assetId) {
  const details = await invoke("get_asset_details", { assetId });
  const date = new Date(details.modified_at * 1000);
  return [date.getUTCFullYear(), date.getUTCMonth() + 1];
}

suite("gallery timeline and select-all selection spec", function () {
  this.timeout(600000);
  before(async () => {
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-timeline-"));
    await fs.mkdir(output, { recursive: true });
    await copyPngFixtures(mediaRoot, "timeline", TOTAL);
    for (let index = 0; index < TOTAL; index++) {
      const month = Math.floor(index / PER_MONTH);
      const time = index >= PER_MONTH * MONTHS
        ? new Date(Date.UTC(2012, 5, 15))
        : new Date(Date.UTC(2023, 11 - month, 15, 12, 0, (PER_MONTH - (index % PER_MONTH)) % 60));
      await fs.utimes(path.join(mediaRoot, `timeline-${index + 1}.png`), time, time);
    }
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 60000 });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
      await fs.writeFile(path.join(output, "failure.json"), JSON.stringify(await browser.execute(() => ({ text: document.body.innerText })), null, 2));
    }
    await browser.releaseActions();
  });
  after(async () => {
    await fs.writeFile(path.join(output, "timings.json"), JSON.stringify(timings, null, 2));
    await invoke("clear_library_data");
    if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
  });

  it("jumps to the oldest month and shows that section's tiles", async () => {
    await slider().waitForExist({ timeout: 30000 });
    assert.equal(await slider().getAttribute("aria-valuemax"), String(MONTHS + 1));
    await valueText("December 2023");
    // The timeline never replaces the position bar.
    assert.equal(await $('[role="scrollbar"][aria-label="Gallery position"]').isExisting(), false);
    timings.endToOldestMs = await jump("End", PER_MONTH * MONTHS);
    await valueText("June 2012");
    for (let index = PER_MONTH * MONTHS; index < TOTAL; index++) {
      const tile = $(`button[data-asset-index="${index}"]`);
      assert.ok(await tile.isDisplayed({ withinViewport: true }), `tile ${index} is visible`);
      assert.deepEqual(await utcMonth(Number(await tile.getAttribute("data-asset-id"))), [2012, 6]);
    }
    await browser.saveScreenshot(path.join(output, "oldest-month.png"));
    timings.arrowUpMs = await jump("ArrowUp", PER_MONTH * (MONTHS - 1));
    await valueText("January 2023");
    const first = $(`button[data-asset-index="${PER_MONTH * (MONTHS - 1)}"]`);
    assert.deepEqual(await utcMonth(Number(await first.getAttribute("data-asset-id"))), [2023, 1]);
    timings.homeMs = await jump("Home", 0);
    await valueText("December 2023");
    timings.pageDownMs = await jump("PageDown", PER_MONTH * MONTHS);
    await valueText("June 2012");
  });

  it("drags the timeline with demand held, then settles on the month under the pointer", async () => {
    const box = await browser.execute(() => {
      const rect = document.querySelector('[role="slider"][aria-label="Timeline"]').getBoundingClientRect();
      return { x: Math.round(rect.left + rect.width / 2), top: Math.round(rect.top), height: Math.round(rect.height) };
    });
    const at = fraction => Math.round(box.top + box.height * fraction);
    await browser.execute(() => {
      const grid = document.querySelector('[data-testid="gallery-grid"]');
      window.__timelinePaused = false;
      window.__timelineObserver = new MutationObserver(() => {
        // Solid renders `true` as an empty attribute and drops `false`.
        if (grid.hasAttribute("data-scroll-paused") && grid.getAttribute("data-scroll-paused") !== "false")
          window.__timelinePaused = true;
      });
      window.__timelineObserver.observe(grid, { attributes: true, attributeFilter: ["data-scroll-paused"] });
    });
    await browser.performActions([{ id: "timeline", type: "pointer", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", duration: 0, x: box.x, y: at(0.1) },
      { type: "pointerDown", button: 0 },
      { type: "pointerMove", duration: 300, x: box.x, y: at(0.3) },
      { type: "pointerMove", duration: 300, x: box.x, y: at(0.52) }
    ] }]);
    await browser.performActions([{ id: "timeline", type: "pointer", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerUp", button: 0 }] }]);
    await browser.releaseActions();
    // 52% of 12,005 is index 6,242: the seventh month, June 2023.
    await valueText("June 2023");
    await browser.waitUntil(() => browser.execute(() => {
      const grid = document.querySelector('[data-testid="gallery-grid"]');
      return !grid.hasAttribute("data-scroll-paused") || grid.getAttribute("data-scroll-paused") === "false";
    }));
    assert.equal(await browser.execute(() => {
      window.__timelineObserver.disconnect();
      return window.__timelinePaused;
    }), true, "page and thumbnail demand was held during the drag");
    await browser.waitUntil(() => browser.execute(() => !document.querySelector('[data-testid="gallery-grid-content"] div[data-asset-index]')), {
      timeout: 15000,
      timeoutMsg: "placeholders remained after the drag"
    });
  });

  it("focuses the rail on a native click so the arrow keys move the timeline", async () => {
    await focusSlider();
    await browser.keys("Home");
    await valueText("December 2023");
    await browser.execute(() => document.activeElement?.blur());
    const box = await browser.execute(() => {
      const rect = document.querySelector('[role="slider"][aria-label="Timeline"]').getBoundingClientRect();
      return { x: Math.round(rect.left + rect.width / 2), top: Math.round(rect.top), height: Math.round(rect.height) };
    });
    // 10% of 12,005 is index 1,200: November 2023.
    await browser.performActions([{ id: "timeline-click", type: "pointer", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", duration: 0, x: box.x, y: Math.round(box.top + box.height * 0.1) },
      { type: "pointerDown", button: 0 },
      { type: "pointerUp", button: 0 }
    ] }]);
    await browser.releaseActions();
    await valueText("November 2023");
    assert.equal(await browser.execute(() => document.activeElement?.getAttribute("role")), "slider");
    await browser.keys("ArrowDown");
    await valueText("October 2023");
  });

  it("opens a last-column tile clicked at its right edge instead of jumping the timeline", async () => {
    await focusSlider();
    await browser.keys("Home");
    await valueText("December 2023");
    const target = await browser.execute(() => {
      const tiles = [...document.querySelectorAll("button[data-asset-index]")].map(tile => ({ index: Number(tile.dataset.assetIndex), rect: tile.getBoundingClientRect() }))
        .filter(({ rect }) => rect.width > 0 && rect.top >= 0 && rect.bottom <= window.innerHeight);
      const right = Math.max(...tiles.map(({ rect }) => rect.right));
      const tile = tiles.find(({ rect }) => rect.right === right);
      const rail = document.querySelector('[role="slider"][aria-label="Timeline"]').getBoundingClientRect();
      return { index: tile.index, x: Math.floor(tile.rect.right) - 2, y: Math.round(tile.rect.top + tile.rect.height / 2), railLeft: rail.left, tileRight: tile.rect.right };
    });
    assert.ok(target.tileRight <= target.railLeft, `last column (${target.tileRight}) clears the rail (${target.railLeft})`);
    await browser.performActions([{ id: "edge-click", type: "pointer", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", duration: 0, x: target.x, y: target.y },
      { type: "pointerDown", button: 0 },
      { type: "pointerUp", button: 0 }
    ] }]);
    await browser.releaseActions();
    await $('[data-testid="lightbox-image"]').waitForExist({ timeout: 15000 });
    await browser.keys("Escape");
    await $('[data-testid="lightbox-image"]').waitForExist({ reverse: true, timeout: 15000 });
    await valueText("December 2023");
  });

  it("selects all results as one range, tags them with one spec write and matches the tag filter", async () => {
    await focusSlider();
    await browser.keys("Home");
    await $('button[data-asset-index="0"]').waitForDisplayed();
    await $('button[aria-label="Enable bulk actions"]').click();
    await $('button[data-asset-index="1"]').click();
    await count(1);
    await browser.keys(["Control", "a"]);
    await count(TOTAL);
    // Ctrl-click inside the range excludes one item.
    const excluded = Number(await $('button[data-asset-index="0"]').getAttribute("data-asset-id"));
    await browser.performActions([{ id: "modifier", type: "key", actions: [{ type: "keyDown", value: "" }] }]);
    await $('button[data-asset-index="0"]').click();
    await browser.releaseActions();
    await count(TOTAL - 1);
    assert.equal(await $('button[data-asset-index="0"]').getAttribute("aria-pressed"), "false");
    assert.equal(await $('button[data-asset-index="1"]').getAttribute("aria-pressed"), "true");
    const started = Date.now();
    await $('#bulk-tag-draft-input').setValue(TAG);
    await browser.keys("Enter");
    await browser.waitUntil(async () => (await $('#bulk-tag-draft-input').getValue()) === "", { timeout: 60000, timeoutMsg: "tag was not saved" });
    timings.selectAllTagMs = Date.now() - started;
    assert.ok(!(await invoke("get_asset_details", { assetId: excluded })).tags.includes(TAG));
    await count(TOTAL - 1);
    await browser.saveScreenshot(path.join(output, "select-all-tagged.png"));

    await $('.filter-input').setValue(TAG);
    await $('button=Search').click();
    await browser.waitUntil(async () => (await $('.workspace-count').getText()) === `${(TOTAL - 1).toLocaleString("en-US")} items`, {
      timeout: 30000,
      timeoutMsg: `Expected ${TOTAL - 1} tagged results`
    });
    // The filter is a new session: the range selection is cleared with a notice.
    await $('[data-testid="bulk-selection-cleared"]').waitForDisplayed({ timeout: 15000 });
    await count(0);
    await browser.saveScreenshot(path.join(output, "selection-cleared.png"));
    await $('button[aria-label="Clear all search filters"]').click();
    await $('button[aria-label="Disable bulk actions"]').click();
  });
});
