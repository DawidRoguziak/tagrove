import { execFile } from "node:child_process";
import { promisify } from "node:util";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

// Opt-in: the ordinary desktop suite should not seed thousands of media files.
const gallerySuite = process.env.MEDIATAGGER_GALLERY_PERF === "1" ? describe : describe.skip;
const IMAGE_COUNT = 2048;
const execFileAsync = promisify(execFile);
let mediaRoot;

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

async function visibleThumbnailsReady() {
  return browser.execute(() => {
    const viewport = document.querySelector(".gallery-scroll").getBoundingClientRect();
    const slots = [
      ...document.querySelectorAll('[data-testid="gallery-grid"] div[style] > [style]')
    ].filter((tile) => {
      const rect = tile.getBoundingClientRect();
      return rect.bottom > viewport.top && rect.top < viewport.bottom && tile.tagName !== "SPAN";
    });
    return (
      slots.length > 0 &&
      slots.every((tile) => {
        const img = tile.querySelector("img");
        return img?.complete && img.naturalWidth > 1 && img.src.endsWith(".jpg");
      })
    );
  });
}

gallerySuite("gallery scrolling performance", function () {
  this.timeout(180000);
  before(async () => {
    // wdio.conf.js validates the native E2E window before any spec runs.
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-gallery-perf-"));
    await copyPngFixtures(mediaRoot, "gallery", IMAGE_COUNT);
    const { gifPath } = await createPlayableFixtures(mediaRoot, "animated");
    for (let index = 1; index < 12; index++) {
      await fs.copyFile(gifPath, path.join(mediaRoot, `animated-${index}.gif`));
    }
    assert.ok((await fs.readdir(mediaRoot)).length <= 10000);
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    const ids = [];
    for (let offset = 0; offset < IMAGE_COUNT; offset += 500) {
      const page = await invoke("list_assets", { offset, limit: 500, tagsAnd: [], tagsNot: [], kind: "image", favoritesOnly: false });
      ids.push(...page.items.map(item => item.id));
    }
    assert.equal(ids.length, IMAGE_COUNT);
    await invoke("merge_asset_tags_bulk", { assetIds: ids, tags: ["gallery-common"] });
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    try {
      await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
    } catch (error) {
      const output = path.resolve("artifacts/gallery-performance");
      await fs.mkdir(output, { recursive: true });
      await browser.saveScreenshot(path.join(output, "startup-failure.png"));
      await fs.writeFile(path.join(output, "startup-failure.json"), JSON.stringify(await browser.execute(() => ({
        text: document.body.innerText, html: document.querySelector("#root")?.innerHTML,
        width: innerWidth, height: innerHeight
      })), null, 2));
      throw error;
    }
  });

  after(async () => {
    await invoke("clear_library_data");
    if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
  });


  it("preserves sessions, scroll anchors and edits across unrelated mutations and evicted pages", async () => {
    await $('#gallery-media-kind input[value="image"]').click();
    await $('button[data-asset-index="0"]').waitForExist({ timeout: 20000 });
    await browser.execute(() => {
      // Tauri's invoke property is immutable. Observe its real custom-protocol
      // requests and cloned responses without replacing the bridge or payloads.
      const original = window.fetch;
      window.queryTrace = [];
      window.fetch = async function(input, options) {
        const url = new URL(String(input));
        const command = decodeURIComponent(url.pathname.slice(1));
        const response = await original.call(this, input, options);
        if (url.protocol === "ipc:" && ["start_asset_query", "get_asset_query_page", "get_asset_query_position"].includes(command)) {
          window.queryTrace.push({ command, args: JSON.parse(options.body), result: await response.clone().json() });
        }
        return response;
      };
    });
    const evidence = [];
    for (const filter of ["", "gallery-common"]) {
      if (filter) await $(".filter-input").setValue(filter);
      else await $(".filter-input").clearValue();
      await $("button=Search").click();
      await browser.waitUntil(() => browser.execute(() => window.queryTrace.some(entry => entry.command === "start_asset_query" && entry.result.status === "ready")));
      const targetIndex = filter ? 384 : 256;
      await scrollToAsset(targetIndex);
      await $(`button[data-asset-index="${targetIndex}"]`).waitForExist({ timeout: 20000 });
      await browser.waitUntil(visibleThumbnailsReady, { timeout: 20000 });
      const anchor = await browser.execute(() => {
        const viewport = document.querySelector(".gallery-scroll").getBoundingClientRect();
        const tile = [...document.querySelectorAll("button[data-asset-index]")].find(tile => {
          const rect = tile.getBoundingClientRect();
          return rect.top >= viewport.top && rect.bottom <= viewport.bottom;
        });
        return { id: Number(tile.dataset.assetId), index: Number(tile.dataset.assetIndex),
          top: tile.getBoundingClientRect().top, scroll: document.querySelector(".gallery-scroll").scrollTop };
      });
      const baseline = await browser.execute(() => {
        const starts = window.queryTrace.filter(entry => entry.command === "start_asset_query" && entry.result.status === "ready");
        return { starts: starts.length, session: starts.at(-1).result.session_id, total: starts.at(-1).result.total };
      });
      assert.equal(baseline.total, IMAGE_COUNT);
      const proofTag = filter ? "filtered-proof" : "unfiltered-proof";
      await $(`button[data-asset-id="${anchor.id}"]`).click();
      await $("#lightbox-tag-draft-input").waitForEnabled();
      await $("#lightbox-tag-draft-input").setValue(proofTag);
      await browser.keys("Enter");
      await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: anchor.id })).tags.includes(proofTag));
      await $('[role="dialog"] button[aria-label="Add to favorites"]').click();
      await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: anchor.id })).is_favorite);
      await $('button[aria-label="Close preview"]').click();
      const readAnchor = () => browser.execute(id => ({
        top: document.querySelector(`button[data-asset-id="${id}"]`)?.getBoundingClientRect().top,
        scroll: document.querySelector(".gallery-scroll").scrollTop
      }), anchor.id);
      let current = await readAnchor();
      assert.ok(Math.abs(current.scroll - anchor.scroll) < 2, "save retains scroll position");
      assert.ok(Math.abs(current.top - anchor.top) < 2, "save retains visible anchor");
      for (const index of [...Array.from({ length: 16 }, (_, page) => page * 128), 2047, 128, anchor.index]) {
        await scrollToAsset(index);
        await $(`button[data-asset-index="${index}"]`).waitForExist({ timeout: 20000 });
        await browser.waitUntil(visibleThumbnailsReady, { timeout: 20000 });
      }
      await browser.execute(scroll => { document.querySelector(".gallery-scroll").scrollTop = scroll; }, anchor.scroll);
      current = await readAnchor();
      assert.ok(Math.abs(current.top - anchor.top) < 2, "evicted page restores the same anchor");
      const trace = await browser.execute(() => window.queryTrace);
      assert.equal(trace.filter(entry => entry.command === "start_asset_query").length, baseline.starts);
      const pages = trace.filter(entry => entry.command === "get_asset_query_page");
      assert.ok(new Set(pages.map(entry => entry.args.offset)).size >= 16);
      assert.ok(pages.every(entry => entry.result.status === "ready" && entry.result.session_id === baseline.session));
      await $(`button[data-asset-id="${anchor.id}"]`).click();
      await browser.waitUntil(async () => (await $('[data-testid="lightbox-tag-panel"]').getText()).includes(proofTag));
      await $('[role="dialog"] button[aria-label="Remove from favorites"]').waitForExist();
      await $('button[aria-label="Close preview"]').click();
      evidence.push({ filter, anchor, baseline, pageOffsets: pages.map(entry => entry.args.offset), proofTag });
      // Each case gets its own trace, including its query start.
      await browser.execute(() => { window.queryTrace = []; });
    }
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForExist({ timeout: 20000 });
    for (const proof of evidence) {
      const details = await invoke("get_asset_details", { assetId: proof.anchor.id });
      assert.ok(details.tags.includes(proof.proofTag));
      assert.equal(details.is_favorite, true);
    }
    // Relevant tag edits must immediately replace the filtered query.
    await $(".filter-input").setValue("filtered-proof");
    await $("button=Search").click();
    await $('button[data-asset-index="0"]').waitForExist({ timeout: 20000 });
    const editedId = evidence[1].anchor.id;
    await $(`button[data-asset-id="${editedId}"]`).click();
    await $('[data-testid="lightbox-tag-panel"] button[aria-label="Remove tag filtered-proof"]').click();
    await browser.waitUntil(async () => !(await invoke("get_asset_details", { assetId: editedId })).tags.includes("filtered-proof"));
    await $('button[aria-label="Close preview"]').click();
    await browser.waitUntil(async () => (await $("body").getText()).includes("No results"));
    // Favorites removal also refreshes membership.
    await $('button[aria-label="Clear all search filters"]').click();
    await $('button[aria-label="Show favorites only"]').click();
    await browser.waitUntil(async () => (await $$("button[data-asset-id]")).length > 0);
    await $(`button[data-asset-id="${editedId}"]`).click();
    await $('[role="dialog"] button[aria-label="Remove from favorites"]').click();
    await browser.waitUntil(async () => !(await invoke("get_asset_details", { assetId: editedId })).is_favorite);
    await $('button[aria-label="Close preview"]').click();
    await browser.waitUntil(async () => !(await $(`button[data-asset-id="${editedId}"]`).isExisting()));
    await $('button[aria-label="Clear all search filters"]').click();
    const output = path.resolve("artifacts/gallery-performance");
    await fs.mkdir(output, { recursive: true });
    await fs.writeFile(path.join(output, "stable-metadata.json"), JSON.stringify(evidence, null, 2));
    await browser.saveScreenshot(path.join(output, "stable-metadata.png"));
  });

  it("loads static GIF tiles, crosses the page cache, and keeps lightbox animation", async () => {
    const results = {};
    // Visit every page, then return through evicted pages. Both passes use the same route.
    const route = [...Array.from({ length: 16 }, (_, page) => page * 128), 2047, 128, 0];
    for (const pass of ["cold", "warm"]) {
      const latencies = [];
      await browser.execute(() => {
        window.galleryPerf = { active: true, frames: [], previous: performance.now() };
        function frame(time) {
          const state = window.galleryPerf;
          state.frames.push(time - state.previous);
          state.previous = time;
          if (state.active) requestAnimationFrame(frame);
        }
        requestAnimationFrame(frame);
      });
      for (const index of route) {
        const started = Date.now();
        await scrollToAsset(index);
        await $(`button[data-asset-index="${index}"]`).waitForExist({ timeout: 20000 });
        await browser.waitUntil(visibleThumbnailsReady, { timeout: 20000, interval: 50 });
        latencies.push(Date.now() - started);
        assert.ok((await $$("button[data-asset-index]")).length < 200);
      }
      const frames = await browser.execute(() => {
        window.galleryPerf.active = false;
        return window.galleryPerf.frames;
      });
      frames.sort((a, b) => a - b);
      latencies.sort((a, b) => a - b);
      results[pass] = {
        frameP95Ms: frames[Math.floor(frames.length * 0.95)],
        thumbnailP95Ms: latencies[Math.floor(latencies.length * 0.95)],
        frameSamples: frames.length
      };
    }
    const output = path.resolve("artifacts/gallery-performance");
    await fs.mkdir(output, { recursive: true });
    await fs.writeFile(path.join(output, "timings.json"), JSON.stringify(results, null, 2));
    await browser.saveScreenshot(path.join(output, "gallery.png"));
    console.log("Gallery timing samples, including WebDriver overhead:", JSON.stringify(results));

    await $('#gallery-media-kind input[value="gif"]').click();
    await $(".filter-input").click();
    await browser.keys("Enter");
    const gif = await $('button[data-asset-id] img[alt$=".gif"]');
    await gif.waitForDisplayed({ timeout: 20000 });
    await browser.waitUntil(visibleThumbnailsReady, { timeout: 20000, interval: 50 });
    assert.ok((await gif.getAttribute("src")).endsWith(".jpg"));
    await gif.click();
    const openedGif = await $('[role="dialog"] img[src$=".gif"]');
    await openedGif.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(
      () =>
        browser.execute(() => {
          const img = document.querySelector('[role="dialog"] img[src$=".gif"]');
          return img?.complete && img.naturalWidth > 1;
        }),
      { timeout: 10000 }
    );
    const bounds = await openedGif.getSize();
    const location = await browser.execute(() => {
      const rect = document
        .querySelector('[role="dialog"] img[src$=".gif"]')
        .getBoundingClientRect();
      return { x: rect.left, y: rect.top };
    });
    const capture = async (index) => {
      const file = path.join(output, `gif-native-frame-${index}.png`);
      await execFileAsync("import", [
        "-window",
        "root",
        "-crop",
        `${Math.round(bounds.width)}x${Math.round(bounds.height)}+${Math.round(location.x)}+${Math.round(location.y)}`,
        file
      ]);
      const { stdout } = await execFileAsync("magick", [file, "-depth", "8", "rgba:-"], {
        encoding: "buffer",
        maxBuffer: 16 * 1024 * 1024
      });
      return stdout;
    };
    const first = await capture(1);
    let advanced = false;
    for (let index = 0; index < 5; index++) {
      await browser.pause(170 + index * 43);
      const next = await capture(index + 2);
      if (!first.equals(next)) {
        advanced = true;
        break;
      }
    }
    assert.equal(advanced, true, "The opened GIF should advance frames on the private display");
    await browser.keys("Escape");
  });
  it("edits the full selection after eviction and filters, and verifies selection IPC", async () => {
    await $('button[aria-label="Clear all search filters"]').click();
    await $('button[data-asset-index="2"]').waitForExist({ timeout: 20000 });
    const ids = await browser.execute(() => [0, 1, 2].map(index => Number(document.querySelector(`button[data-asset-index="${index}"]`).dataset.assetId)));
    const summaries = await invoke("get_asset_summaries_by_ids", { assetIds: [ids[2], 999999, ids[0]] });
    assert.deepEqual(summaries.map(item => item.id), [ids[2], ids[0]]);
    await assert.rejects(() => invoke("get_asset_summaries_by_ids", { assetIds: Array(257).fill(ids[0]) }), /256/);

    await $('button[aria-label="Enable bulk actions"]').click();
    // Dispatch the same modifier-click events consumed by gallery tiles.
    await browser.execute(selected => {
      selected.forEach((id, index) => document.querySelector(`button[data-asset-id="${id}"]`)
        .dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: index > 0 })));
    }, ids);
    await browser.waitUntil(async () => (await $('[data-testid="bulk-header-panel"]').getText()).includes("Selected: 3"));
    await $('#bulk-group-key-input').setValue("selection-proof");
    for (let index = 128; index < 2048; index += 128) {
      await scrollToAsset(index);
      await $(`button[data-asset-index="${index}"]`).waitForExist({ timeout: 20000 });
    }
    assert.equal(await $('#bulk-group-key-input').getValue(), "selection-proof");
    await $('.filter-input').setValue("absent-selection-proof");
    await $('button=Search').click();
    await browser.waitUntil(async () => (await $('body').getText()).includes("No results"));
    assert.equal(await $('#bulk-group-key-input').getValue(), "selection-proof");
    await $('#bulk-tag-draft-input').setValue("bulk-proof");
    await browser.keys("Enter");
    await browser.waitUntil(async () => {
      const details = await Promise.all(ids.map(assetId => invoke("get_asset_details", { assetId })));
      return details.every(item => item.tags.includes("bulk-proof"));
    });
    await $('button=Apply group').click();
    await browser.waitUntil(async () => {
      const items = await invoke("get_asset_summaries_by_ids", { assetIds: ids });
      return items.length === 3 && items.every((item, index) => item.media_group_key === "selection-proof" && item.media_group_order === index + 1);
    });
    await $('[data-testid="bulk-header-panel"] button[aria-pressed]').click();
    await browser.waitUntil(async () => (await invoke("get_asset_summaries_by_ids", { assetIds: ids })).every(item => item.is_favorite));
    await browser.saveScreenshot(path.resolve("artifacts/gallery-performance/selection.png"));
    await $('button[aria-label="Disable bulk actions"]').click();
  });

});
