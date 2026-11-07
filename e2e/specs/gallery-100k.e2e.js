import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

// Grid-performance stage 10 (open topic 5): the stage 04 measurements on a real 100k
// collection, kept as an opt-in spec. Shift-click through a materialized range and through
// the whole result, and a refresh deep in the list. Synthetic PNG copies in a fresh root only.
const suite = process.env.MEDIATAGGER_GALLERY_100K === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/gallery-100k");
const TOTAL = 100_000;
const MATERIALIZE_LIMIT = 10_000; // src/features/gallery/createSelection.ts
const SHIFT_CLICK_LIMIT_MS = 200; // stage 04 criterion
const FRAMES = 240;
const PAGE = 128;
let mediaRoot;
const timings = {};

async function invoke(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (response.error) throw new Error(response.error);
  return response.value;
}
/** Commands that outlive the WebDriver script timeout run in the page and are polled. */
async function invokeLong(command, payload = {}, timeoutMs = 20 * 60 * 1000) {
  const slot = `${command}-${Date.now()}`;
  await browser.execute((name, args, key) => {
    window.gallery100kLong ??= {};
    window.gallery100kLong[key] = { done: false };
    window.__TAURI_INTERNALS__.invoke(name, args).then(
      value => { window.gallery100kLong[key] = { done: true, value }; },
      error => { window.gallery100kLong[key] = { done: true, error: String(error) }; }
    );
  }, command, payload, slot);
  await browser.waitUntil(() => browser.execute(key => window.gallery100kLong[key].done, slot), {
    timeout: timeoutMs, interval: 500, timeoutMsg: `${command} did not finish`
  });
  const response = await browser.execute(key => window.gallery100kLong[key], slot);
  if (response.error) throw new Error(response.error);
  return response.value;
}
/**
 * Logs every IPC command the app sends from now on. Tauri's `invoke` cannot be replaced, so
 * this wraps the custom-protocol `fetch` bridge, as `grid-baseline.e2e.js` does.
 */
async function recordIpc() {
  await browser.execute(() => {
    window.gallery100kIpc = [];
    if (window.gallery100kFetch) return;
    const original = window.fetch;
    window.gallery100kFetch = original;
    window.fetch = function (input, options) {
      const url = new URL(String(input), location.href);
      if (url.protocol === "ipc:") {
        const command = decodeURIComponent(url.pathname.slice(1));
        let args;
        try {
          const body = JSON.parse(options?.body ?? "{}");
          if (command === "start_asset_query") args = { anchorOffset: body.anchorOffset ?? null };
          if (command === "get_asset_query_page") args = { offset: body.offset };
        } catch {
          args = null;
        }
        window.gallery100kIpc.push({ command, args });
      }
      return original.call(this, input, options);
    };
  });
}
const ipc = () => browser.execute(() => window.gallery100kIpc.map(({ command, args }) => ({ command, args })));
const clearIpc = () => browser.execute(() => { window.gallery100kIpc.length = 0; });
async function count(expected) {
  await browser.waitUntil(async () => (await $('[data-testid="bulk-header-panel"]').getText()).includes(`Selected: ${expected}`), {
    timeout: 30000, timeoutMsg: `Expected ${expected} selected`
  });
}
/** Scrolls the native gallery scroller (100k stays below the windowed height limit). */
async function scrollToFraction(fraction) {
  await browser.execute(value => {
    const scroller = document.querySelector(".gallery-scroll");
    scroller.scrollTop = Math.round((scroller.scrollHeight - scroller.clientHeight) * value);
  }, fraction);
  return settled();
}
/** Waits until every tile in the viewport has data, then returns the visible index range. */
async function settled() {
  let visible;
  await browser.waitUntil(async () => {
    visible = await browser.execute(() => {
      const view = document.querySelector(".gallery-scroll").getBoundingClientRect();
      const inView = el => { const r = el.getBoundingClientRect(); return r.bottom > view.top && r.top < view.bottom; };
      const grid = document.querySelector('[data-testid="gallery-grid"]');
      const placeholders = [...document.querySelectorAll('div[data-asset-index]')].filter(inView).length;
      const tiles = [...document.querySelectorAll('button[data-asset-index]')].filter(inView)
        .map(el => Number(el.dataset.assetIndex));
      return { placeholders, paused: grid.hasAttribute("data-scroll-paused"), first: Math.min(...tiles), last: Math.max(...tiles), tiles: tiles.length };
    });
    return visible.tiles > 0 && !visible.placeholders && !visible.paused;
  }, { timeout: 30000, timeoutMsg: "visible tiles did not load" });
  return visible;
}
/** Native click (Shift optional); returns ms from the click to the bulk counter showing `expected`. */
async function timedClick(index, expected, shift = false) {
  await browser.execute(target => {
    const panel = document.querySelector('[data-testid="bulk-header-panel"]');
    window.gallery100kClick = { clicked: 0, counted: 0 };
    const record = window.gallery100kClick;
    document.addEventListener("click", event => {
      if (event.target instanceof Element && event.target.closest(`button[data-asset-index="${target.index}"]`)) record.clicked = performance.now();
    }, { capture: true, once: true });
    const observer = new MutationObserver(() => {
      if (!record.counted && panel.textContent.includes(`Selected: ${target.expected}`)) {
        record.counted = performance.now();
        observer.disconnect();
      }
    });
    observer.observe(panel, { childList: true, subtree: true, characterData: true });
  }, { index, expected });
  if (shift) await browser.performActions([{ id: "modifier", type: "key", actions: [{ type: "keyDown", value: "" }] }]);
  await $(`button[data-asset-index="${index}"]`).click();
  if (shift) await browser.releaseActions();
  await count(expected);
  const record = await browser.execute(() => window.gallery100kClick);
  assert.ok(record.clicked && record.counted, "click and counter were observed");
  return Math.round((record.counted - record.clicked) * 10) / 10;
}

suite("100k collection: Shift-click ranges and a refresh deep in the list", function () {
  this.timeout(40 * 60 * 1000);
  before(async () => {
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-100k-"));
    await fs.mkdir(output, { recursive: true });
    let started = Date.now();
    await copyPngFixtures(mediaRoot, "large", TOTAL);
    timings.copyFilesMs = Date.now() - started;
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: mediaRoot });
    started = Date.now();
    await invokeLong("rescan_all_roots");
    timings.scanMs = Date.now() - started;
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 120000 });
    await browser.waitUntil(async () => (await $('.workspace-count').getText()) === `${TOTAL} items`, {
      timeout: 60000, timeoutMsg: `Expected ${TOTAL} items`
    });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
      await fs.writeFile(path.join(output, "failure.json"), JSON.stringify(await browser.execute(() => ({ text: document.body.innerText, ipc: window.gallery100kIpc ?? null })), null, 2));
    }
    await browser.releaseActions();
  });
  after(async () => {
    await fs.writeFile(path.join(output, "timings.json"), JSON.stringify(timings, null, 2));
    try {
      await invokeLong("clear_library_data");
    } finally {
      if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
    }
  });

  it("resolves a 10,000-position Shift-click deep in the list with one binary ID read", async () => {
    await $('button[aria-label="Enable bulk actions"]').click();
    await count(0);
    const deep = await scrollToFraction(0.5);
    const anchor = deep.first;
    assert.ok(anchor > 40_000 && anchor < 60_000, `anchor ${anchor} is deep in the list`);
    await recordIpc();
    await timedClick(anchor, 1);
    // Bring the far end of the range into view without leaving the 24-page cache window.
    const end = anchor + MATERIALIZE_LIMIT - 1;
    await browser.execute(index => {
      const scroller = document.querySelector(".gallery-scroll");
      const rows = [...document.querySelectorAll('[data-gallery-row]')].filter(row => !row.hidden)
        .sort((a, b) => Number(a.dataset.galleryRow) - Number(b.dataset.galleryRow));
      const [top, next] = rows;
      const columns = top.querySelectorAll('[data-asset-index]').length;
      const stride = next.getBoundingClientRect().top - top.getBoundingClientRect().top;
      scroller.scrollTop += (Math.floor(index / columns) - Number(top.dataset.galleryRow)) * stride;
    }, end);
    await settled();
    await $(`button[data-asset-index="${end}"]`).waitForDisplayed({ timeout: 30000 });
    await clearIpc();
    timings.shiftClick10kMs = await timedClick(end, MATERIALIZE_LIMIT, true);
    const commands = (await ipc()).map(entry => entry.command);
    assert.equal(commands.filter(command => command === "get_asset_query_id_range").length, 1, `one binary ID read: ${commands}`);
    assert.equal(commands.filter(command => command === "get_asset_query_page").length, 0, "no page reads for the range");
    assert.ok(timings.shiftClick10kMs < SHIFT_CLICK_LIMIT_MS, `Shift-click 10k took ${timings.shiftClick10kMs} ms`);
    await browser.keys("Escape");
    await count(0);
  });

  it("selects the whole 100k result by Shift-click without reading IDs", async () => {
    await scrollToFraction(0);
    await recordIpc();
    await timedClick(0, 1);
    await scrollToFraction(1);
    await $(`button[data-asset-index="${TOTAL - 1}"]`).waitForDisplayed({ timeout: 30000 });
    await clearIpc();
    timings.shiftClick100kMs = await timedClick(TOTAL - 1, TOTAL, true);
    const commands = (await ipc()).map(entry => entry.command);
    assert.equal(commands.filter(command => command === "get_asset_query_id_range").length, 0, `no ID reads: ${commands}`);
    assert.ok(timings.shiftClick100kMs < SHIFT_CLICK_LIMIT_MS, `Shift-click 100k took ${timings.shiftClick100kMs} ms`);
    await browser.saveScreenshot(path.join(output, "shift-click-100k.png"));
    await browser.keys("Escape");
    await count(0);
    await $('button[aria-label="Disable bulk actions"]').click();
  });

  it("refreshes at index ~50,000 with one anchored query and no placeholders", async () => {
    const before = await scrollToFraction(0.5);
    await browser.pause(500);
    await recordIpc();
    // Sample every frame from the click: placeholders in view and the first visible tile.
    await browser.execute(frames => {
      const scroller = document.querySelector(".gallery-scroll");
      const view = scroller.getBoundingClientRect();
      const inView = el => { const r = el.getBoundingClientRect(); return r.bottom > view.top && r.top < view.bottom; };
      window.gallery100kFrames = [];
      const sample = () => {
        const tiles = [...document.querySelectorAll('button[data-asset-index]')].filter(inView).map(el => Number(el.dataset.assetIndex));
        window.gallery100kFrames.push({ placeholders: [...document.querySelectorAll('div[data-asset-index]')].filter(inView).length,
          first: tiles.length ? Math.min(...tiles) : -1 });
        if (window.gallery100kFrames.length < frames) requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    }, FRAMES);
    await $('button=Search').click();
    await browser.waitUntil(() => browser.execute(frames => window.gallery100kFrames.length >= frames, FRAMES), {
      timeout: 30000, timeoutMsg: "frame sampling did not finish"
    });
    const log = await ipc();
    const frames = await browser.execute(() => window.gallery100kFrames);
    const starts = log.filter(entry => entry.command === "start_asset_query");
    const anchorPage = Math.floor(before.first / PAGE) * PAGE;
    const pages = log.filter(entry => entry.command === "get_asset_query_page").map(entry => entry.args?.offset);
    timings.refresh = { anchor: before.first, visibleLast: before.last, anchorPage, starts: starts.map(entry => entry.args), pages,
      placeholderFrames: frames.filter(frame => frame.placeholders).length, frames: frames.length };
    assert.equal(starts.length, 1, "one start_asset_query");
    assert.equal(starts[0].args.anchorOffset, before.first, "the refresh is anchored at the first visible index");
    // The reply carries the anchor's page; only neighbouring pages of the visible and
    // overscan rows may follow, never a sequential walk from offset 0.
    assert.ok(!pages.includes(anchorPage), `anchor page ${anchorPage} is not read again: ${pages}`);
    assert.ok(pages.length <= 2 && pages.every(offset => Math.abs(offset - anchorPage) <= 2 * PAGE), `neighbouring pages only: ${pages}`);
    assert.equal(timings.refresh.placeholderFrames, 0, "no placeholders in view during the refresh");
    assert.ok(frames.every(frame => frame.first === before.first), "the view stays on the same tiles");
  });
});
