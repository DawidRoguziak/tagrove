import { execFile } from "node:child_process";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { createPlayableFixtures } from "../fixtures.js";
import { e2eWindowTitle } from "../build-e2e.js";

// Grid-performance baseline, step 0 of
// docs/plans/grid-performance/07-verification-and-docs.md. Opt-in: it generates
// 20,000 JPEGs and runs for several minutes. The three cases share one library
// and must run in order, as a whole spec. There are no thresholds; results are
// raw timings for comparison between runs on the same host and build profile.
const baselineSuite = process.env.MEDIATAGGER_GRID_BASELINE === "1" ? describe : describe.skip;
const execFileAsync = promisify(execFile);
const ffmpeg = process.env.FFMPEG_PATH ?? "ffmpeg";
const SCAN_FILES = 20000;
const BROWSE_FILES = 240;
const BURST_COUNT = 20;
const DELAYED_MS = 1000;
const OP_TIMEOUT_MS = 10000;
const output = path.resolve(
  "artifacts/grid-baseline",
  new Date().toISOString().replace(/[:.]/g, "-")
);
let root;
const results = {
  measurement: "grid-performance baseline; debug E2E build; synthetic media",
  scenario: {},
  warm: {},
  cold: {}
};

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

// Long commands outlive WebDriver's script timeout, and a retried
// executeAsync would start them twice. Start in the page, then poll.
async function invokeLong(command, payload = {}, timeoutMs = 30 * 60 * 1000) {
  const key = `${command}-${Date.now()}`;
  await browser.execute((name, args, slot) => {
    window.gridLong ??= {};
    window.gridLong[slot] = { done: false };
    window.__TAURI_INTERNALS__.invoke(name, args).then(
      (value) => { window.gridLong[slot] = { done: true, value }; },
      (error) => { window.gridLong[slot] = { done: true, error: String(error) }; }
    );
  }, command, payload, key);
  await browser.waitUntil(() => browser.execute((slot) => window.gridLong[slot].done, key), {
    timeout: timeoutMs,
    interval: 500,
    timeoutMsg: `${command} did not finish`
  });
  const response = await browser.execute((slot) => window.gridLong[slot], key);
  if (response.error) throw new Error(response.error);
  return response.value;
}

async function ffmpegFrames(source, count, pattern) {
  await execFileAsync(ffmpeg, [
    "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", source,
    "-frames:v", String(count), "-q:v", "5", "-y", pattern
  ], { maxBuffer: 16 * 1024 * 1024 });
}

async function setMtimes(dir, names, timeFor) {
  for (let start = 0; start < names.length; start += 500) {
    await Promise.all(names.slice(start, start + 500).map((name, offset) => {
      const time = timeFor(start + offset);
      return fs.utimes(path.join(dir, name), time, time);
    }));
  }
}

// Records IPC requests made through the custom-protocol fetch bridge. Only
// timings, command names and the arguments needed for matching are kept.
async function installTrace() {
  await browser.execute(() => {
    if (window.gridTrace) return;
    const traced = new Set([
      "start_asset_query", "get_asset_query_page", "set_asset_favorite", "set_asset_tags",
      "get_asset_details", "scan_folder"
    ]);
    const original = window.fetch;
    window.gridTrace = { calls: [], counts: {} };
    window.fetch = async function (input, options) {
      const url = new URL(String(input), location.href);
      if (url.protocol !== "ipc:") return original.call(this, input, options);
      const command = decodeURIComponent(url.pathname.slice(1));
      window.gridTrace.counts[command] = (window.gridTrace.counts[command] ?? 0) + 1;
      if (!traced.has(command)) return original.call(this, input, options);
      const entry = { command, t0: performance.now(), t1: null, ok: null, status: null };
      try {
        const args = typeof options?.body === "string" ? JSON.parse(options.body) : {};
        if (command === "set_asset_favorite") entry.args = { assetId: args.assetId, isFavorite: args.isFavorite };
        if (command === "set_asset_tags") entry.args = { assetId: args.assetId, tags: args.tags };
        if (command === "get_asset_query_page") entry.args = { offset: args.offset };
      } catch {
        entry.args = null;
      }
      window.gridTrace.calls.push(entry);
      try {
        const response = await original.call(this, input, options);
        entry.t1 = performance.now();
        entry.ok = response.ok;
        entry.status = response.headers.get("Tauri-Response") ?? String(response.status);
        if (command === "start_asset_query" || command === "get_asset_query_page") {
          const body = await response.clone().json().catch(() => null);
          entry.result = body?.status ?? null;
          entry.total = body?.total ?? null;
        }
        return response;
      } catch (error) {
        entry.t1 = performance.now();
        entry.ok = false;
        entry.error = String(error);
        throw error;
      }
    };
  });
}

async function waitForGallery() {
  await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 60000 });
  await installTrace();
}

async function closePreview() {
  await $('button[aria-label="Close preview"]').click();
  await browser.waitUntil(() => browser.execute(() => !document.querySelector('[role="dialog"][data-lightbox-kind]')), {
    timeout: 15000,
    timeoutMsg: "preview must close"
  });
}

function stats(values) {
  const sorted = values.filter((value) => typeof value === "number").sort((a, b) => a - b);
  if (!sorted.length) return { n: 0 };
  const pick = (q) => sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * q))];
  const round = (value) => Math.round(value * 10) / 10;
  return { n: sorted.length, min: round(sorted[0]), p50: round(pick(0.5)), p95: round(pick(0.95)), max: round(sorted.at(-1)) };
}

// In-page: watches one tag addition and one favorite click on the open preview.
function armBurst(tag, timeoutMs) {
  const state = {
    tag, armedAt: performance.now(), enterAt: null, clickAt: null,
    tagShownAt: null, favoriteShownAt: null, finished: false, active: true
  };
  window.burst = state;
  const tagSelector = `[data-testid="lightbox-tag-panel"] button[aria-label="Remove tag ${tag}"]`;
  const onKey = (event) => {
    if (event.key === "Enter" && event.target?.id === "lightbox-tag-draft-input" && state.enterAt === null) {
      state.enterAt = performance.now();
    }
  };
  const onClick = (event) => {
    if (event.target?.closest?.('button[aria-label="Add to favorites"]') && state.clickAt === null) {
      state.clickAt = performance.now();
    }
  };
  document.addEventListener("keydown", onKey, true);
  document.addEventListener("click", onClick, true);
  const frame = () => {
    const now = performance.now();
    if (state.tagShownAt === null && document.querySelector(tagSelector)) state.tagShownAt = now;
    if (state.favoriteShownAt === null && document.querySelector('[role="dialog"] button[aria-label="Remove from favorites"]')) {
      state.favoriteShownAt = now;
    }
    const lastAction = Math.max(state.enterAt ?? state.armedAt, state.clickAt ?? state.armedAt);
    if ((state.tagShownAt !== null && state.favoriteShownAt !== null) || now > lastAction + timeoutMs || now > state.armedAt + 3 * timeoutMs) {
      state.finished = true;
    }
    if (!state.active || state.finished) {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("click", onClick, true);
      const dialog = document.querySelector('[role="dialog"][data-lightbox-kind]');
      const text = dialog?.innerText ?? "";
      state.uiErrors = ["Could not save change", "favorite change was not saved", "Metadata is busy", "Could not load tags"]
        .filter((needle) => text.includes(needle));
      return;
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

// In-page: scrolls like a decaying fling (or jumps), then samples every frame
// until the visible slots are complete and at least one second has passed.
function flingAndSettle(spec, done) {
  const grid = document.querySelector('[data-testid="gallery-grid"]');
  const scroller = grid.closest(".gallery-scroll");
  const visible = () => {
    const viewport = scroller.getBoundingClientRect();
    let tiles = 0;
    let placeholders = 0;
    let loaded = 0;
    for (const element of grid.querySelectorAll("[data-asset-index]")) {
      const rect = element.getBoundingClientRect();
      if (rect.bottom <= viewport.top || rect.top >= viewport.bottom) continue;
      if (element.tagName !== "BUTTON") {
        placeholders++;
        continue;
      }
      tiles++;
      const image = element.querySelector("img");
      if (image?.complete && image.naturalWidth > 1 && !image.src.startsWith("data:")) loaded++;
    }
    return { tiles, placeholders, loaded, complete: tiles > 0 && placeholders === 0 && loaded === tiles };
  };
  const longTasks = [];
  let observer = null;
  if (PerformanceObserver.supportedEntryTypes?.includes("longtask")) {
    observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) longTasks.push(entry.duration);
    });
    observer.observe({ type: "longtask" });
  }
  const startTop = scroller.scrollTop;
  const maxTop = scroller.scrollHeight - scroller.clientHeight;
  const result = { spec, startTop, maxTop, flingFrames: 0, pausedFrames: 0, frameGapsAfterStop: [] };
  let velocity = spec.velocity ?? 0;
  let previous = performance.now();
  let stopAt = null;
  let completeAt = null;
  const finish = () => {
    observer?.disconnect();
    const state = visible();
    result.endTop = scroller.scrollTop;
    result.distance = Math.abs(result.endTop - startTop);
    result.settleMs = completeAt === null ? null : completeAt - stopAt;
    result.complete = state;
    result.longTasks = observer ? longTasks : null;
    result.maxFrameGapAfterStop = Math.max(0, ...result.frameGapsAfterStop);
    result.mountedTiles = grid.querySelectorAll("button[data-asset-index]").length;
    result.gridNodes = grid.querySelectorAll("*").length;
    done(result);
  };
  const frame = (now) => {
    const gap = now - previous;
    previous = now;
    if (stopAt === null) {
      const paused = grid.hasAttribute("data-scroll-paused") && grid.getAttribute("data-scroll-paused") !== "false";
      if (paused) result.pausedFrames++;
      if (spec.jumpTo !== undefined) {
        scroller.scrollTop = Math.round(maxTop * spec.jumpTo);
        velocity = 0;
      } else {
        scroller.scrollTop = Math.max(0, Math.min(maxTop, scroller.scrollTop + velocity * gap * spec.direction));
        velocity *= spec.decay ** (gap / 16.67);
      }
      result.flingFrames++;
      const atBound = scroller.scrollTop <= 0 || scroller.scrollTop >= maxTop;
      if (velocity < 0.02 || atBound) stopAt = now;
      requestAnimationFrame(frame);
      return;
    }
    result.frameGapsAfterStop.push(Math.round(gap * 10) / 10);
    if (completeAt === null && visible().complete) completeAt = now;
    const elapsed = now - stopAt;
    if ((completeAt !== null && elapsed >= 1000) || elapsed > spec.timeoutMs) {
      finish();
      return;
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

// In-page: jumps to a cold position and times each visible tile's first real
// thumbnail load from the moment the new position was applied.
function coldJump(fraction, timeoutMs, done) {
  const grid = document.querySelector('[data-testid="gallery-grid"]');
  const scroller = grid.closest(".gallery-scroll");
  const loads = new Map();
  const onLoad = (event) => {
    const image = event.target;
    if (!(image instanceof HTMLImageElement) || image.src.startsWith("data:")) return;
    const id = image.closest("button[data-asset-id]")?.dataset.assetId;
    if (id && !loads.has(id)) loads.set(id, performance.now());
  };
  grid.addEventListener("load", onLoad, true);
  scroller.scrollTop = Math.round((scroller.scrollHeight - scroller.clientHeight) * fraction);
  const appliedAt = performance.now();
  let tilesAt = null;
  const frame = () => {
    const now = performance.now();
    const viewport = scroller.getBoundingClientRect();
    const slots = [...grid.querySelectorAll("[data-asset-index]")].filter((element) => {
      const rect = element.getBoundingClientRect();
      return rect.bottom > viewport.top && rect.top < viewport.bottom;
    });
    const tiles = slots.filter((element) => element.tagName === "BUTTON");
    if (tilesAt === null && slots.length && tiles.length === slots.length) tilesAt = now;
    const ready = tiles.length > 0 && tiles.length === slots.length && tiles.every((tile) => {
      const image = tile.querySelector("img");
      return image?.complete && image.naturalWidth > 1 && !image.src.startsWith("data:");
    });
    if (ready || now - appliedAt > timeoutMs) {
      grid.removeEventListener("load", onLoad, true);
      done({
        fraction,
        ready,
        visibleTiles: tiles.length,
        tilesMs: tilesAt === null ? null : tilesAt - appliedAt,
        allThumbnailsMs: ready ? now - appliedAt : null,
        perTileMs: tiles.map((tile) => {
          const loadedAt = loads.get(tile.dataset.assetId);
          return loadedAt === undefined ? null : Math.round((loadedAt - appliedAt) * 10) / 10;
        })
      });
      return;
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

baselineSuite("grid-performance baseline", function () {
  this.timeout(30 * 60 * 1000);
  let browseRoot;
  let scanRoot;

  before(async () => {
    // wdio.conf.js already checked the live window title; check again before
    // clearing the isolated library.
    assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
    await browser.setWindowSize(1440, 1000);
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-grid-baseline-"));
    browseRoot = path.join(root, "browse");
    scanRoot = path.join(root, "scan");
    await fs.mkdir(browseRoot);
    await fs.mkdir(scanRoot);
    const generated = Date.now();
    await ffmpegFrames("testsrc2=size=320x240:rate=30", SCAN_FILES, path.join(scanRoot, "scan-%05d.jpg"));
    await ffmpegFrames("testsrc=size=320x240:rate=30", BROWSE_FILES, path.join(browseRoot, "browse-%03d.jpg"));
    await createPlayableFixtures(browseRoot, "clip");
    results.fixture = { scanFiles: SCAN_FILES, browseFiles: BROWSE_FILES, clips: 3, size: "320x240", generateMs: Date.now() - generated };
    // Scan files spread over 2015-2023, oldest first by name. Browse images
    // keep their current time, so they stay first in the gallery while the
    // scan adds older rows. Clips are the oldest rows.
    const scanNames = (await fs.readdir(scanRoot)).sort();
    assert.equal(scanNames.length, SCAN_FILES);
    const from = Date.UTC(2015, 0, 1) / 1000;
    const span = 9 * 365 * 24 * 3600;
    await setMtimes(scanRoot, scanNames, (index) => from + Math.floor((index * span) / SCAN_FILES));
    const clipNames = (await fs.readdir(browseRoot)).filter((name) => name.startsWith("clip"));
    await setMtimes(browseRoot, clipNames, () => Date.UTC(2014, 0, 1) / 1000);

    await invoke("clear_library_data");
    await invokeLong("clear_all_thumbnails");
    await invokeLong("scan_folder", { path: browseRoot });
    await browser.refresh();
    await waitForGallery();
  });

  after(async () => {
    await fs.mkdir(output, { recursive: true });
    await fs.writeFile(path.join(output, "results.json"), JSON.stringify(results, null, 2));
    console.log(`[grid-baseline] results ${path.join(output, "results.json")}`);
    try {
      assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
      await invokeLong("clear_library_data");
      await invokeLong("clear_all_thumbnails");
    } finally {
      if (root) await fs.rm(root, { recursive: true, force: true });
    }
  });

  it("adds tags and favorites while 20,000-file scans run", async () => {
    const targets = await browser.execute((count) =>
      [...document.querySelectorAll("button[data-asset-index]")]
        .sort((a, b) => Number(a.dataset.assetIndex) - Number(b.dataset.assetIndex))
        .slice(0, count)
        .map((tile) => Number(tile.dataset.assetId)), BURST_COUNT);
    assert.equal(targets.length, BURST_COUNT);
    for (const id of targets) {
      const details = await invoke("get_asset_details", { assetId: id });
      assert.equal(details.kind, "image");
      assert.equal(details.is_favorite, false);
    }

    // A 20,000-file scan takes about two seconds here, while one preview
    // operation takes about a second of WebDriver input. To keep indexing
    // writes running for all 20 operations, the page alternates scanning the
    // same 20,000 new files and removing that root again. Every operation is
    // labelled with the phase it fell into; the loop ends after a scan.
    await browser.execute((scanPath) => {
      const load = { stop: false, done: false, error: null, phases: [] };
      window.gridLoad = load;
      const call = window.__TAURI_INTERNALS__.invoke;
      (async () => {
        try {
          for (;;) {
            const scan = { type: "scan", startedAt: performance.now(), endedAt: null };
            load.phases.push(scan);
            const summary = await call("scan_folder", { path: scanPath });
            scan.endedAt = performance.now();
            scan.indexed = summary.indexed;
            if (load.stop) break;
            const remove = { type: "remove", startedAt: performance.now(), endedAt: null };
            load.phases.push(remove);
            const removed = await call("remove_scan_root", { path: scanPath });
            remove.endedAt = performance.now();
            remove.removed = removed.removed_assets;
          }
        } catch (error) {
          load.error = String(error);
        }
        load.done = true;
      })();
    }, scanRoot);
    // Let discovery reach the first indexing batches before the first click.
    await browser.pause(300);

    const bursts = [];
    for (const [index, id] of targets.entries()) {
      const tag = `burst-${index}`;
      const clicked = Date.now();
      await $(`button[data-asset-id="${id}"]`).click();
      const input = await $("#lightbox-tag-draft-input");
      await input.waitForEnabled({ timeout: 60000 });
      const lightboxReadyMs = Date.now() - clicked;
      const heart = await $('[role="dialog"] button[aria-label="Add to favorites"]');
      await heart.waitForExist({ timeout: 10000 });
      await browser.execute(armBurst, tag, OP_TIMEOUT_MS);
      await input.setValue(tag);
      await browser.keys("Enter");
      await heart.click();
      // A click on a disabled or hidden heart never reaches the page; keep
      // that apart from application failures.
      const clickState = await browser.execute(() => ({
        delivered: window.burst.clickAt !== null,
        heartDisabled: document.querySelector('[role="dialog"] button[aria-label="Add to favorites"]')?.disabled ?? null
      }));
      await browser.waitUntil(() => browser.execute(() => window.burst.finished), {
        timeout: 4 * OP_TIMEOUT_MS,
        interval: 100
      });
      const record = await browser.execute(() => {
        window.burst.active = false;
        return { ...window.burst };
      });
      bursts.push({ index, id, lightboxReadyMs, clickState, ...record });
      await closePreview();
    }

    await browser.execute(() => {
      window.gridLoad.stop = true;
    });
    await browser.waitUntil(() => browser.execute(() => window.gridLoad.done), {
      timeout: 20 * 60 * 1000,
      interval: 500,
      timeoutMsg: "the scan/remove load did not stop"
    });
    const load = await browser.execute(() => window.gridLoad);
    assert.equal(load.error, null);
    assert.equal(load.phases.at(-1).type, "scan");
    const trace = await browser.execute(() => window.gridTrace);
    const phaseAt = (time) =>
      load.phases.find((phase) => time >= phase.startedAt && time <= phase.endedAt)?.type ?? "between";
    const scanPhases = load.phases.filter((phase) => phase.type === "scan");
    const inScan = (call) => phaseAt(call.t0) === "scan";

    const operations = [];
    for (const burst of bursts) {
      const details = await invoke("get_asset_details", { assetId: burst.id });
      const favoriteCalls = trace.calls.filter((call) =>
        call.command === "set_asset_favorite" && call.args?.assetId === burst.id);
      const tagCalls = trace.calls.filter((call) =>
        call.command === "set_asset_tags" && call.args?.assetId === burst.id && call.args.tags?.includes(burst.tag));
      for (const [kind, actionAt, shownAt, calls, persisted] of [
        ["tag", burst.enterAt, burst.tagShownAt, tagCalls, details.tags.includes(burst.tag)],
        ["favorite", burst.clickAt, burst.favoriteShownAt, favoriteCalls, details.is_favorite === true]
      ]) {
        const uiMs = actionAt === null || shownAt === null ? null : shownAt - actionAt;
        const call = calls[0];
        // The favorite state is published after the backend succeeds. A tag
        // chip appears at once as a pending intent, so its save is confirmed
        // only when the IPC call returns.
        const confirmedMs = kind === "tag"
          ? (actionAt !== null && call?.t1 != null ? call.t1 - actionAt : null)
          : uiMs;
        operations.push({
          kind,
          index: burst.index,
          inputDelivered: actionAt !== null,
          phase: actionAt === null ? null : phaseAt(actionAt),
          ipcIssued: calls.length > 0,
          ipcCount: calls.length,
          ipcMs: call?.t1 != null ? call.t1 - call.t0 : null,
          ipcOk: call?.ok ?? null,
          uiMs,
          confirmedMs,
          persisted,
          uiErrors: burst.uiErrors,
          failed: actionAt !== null && (confirmedMs === null || !persisted),
          delayed: confirmedMs !== null && persisted && confirmedMs > DELAYED_MS
        });
      }
    }
    const summarize = (kind) => {
      const subset = operations.filter((operation) => operation.kind === kind && operation.inputDelivered);
      const scanSubset = subset.filter((operation) => operation.phase === "scan");
      return {
        attempted: subset.length,
        byPhase: Object.fromEntries(["scan", "remove", "between"].map((phase) =>
          [phase, subset.filter((operation) => operation.phase === phase).length])),
        failed: subset.filter((operation) => operation.failed).length,
        failedDuringScan: scanSubset.filter((operation) => operation.failed).length,
        delayedDuringScan: scanSubset.filter((operation) => operation.delayed).length,
        noIpc: subset.filter((operation) => !operation.ipcIssued).length,
        delayed: subset.filter((operation) => operation.delayed).length,
        uiMs: stats(subset.map((operation) => operation.uiMs)),
        confirmedMs: stats(subset.map((operation) => operation.confirmedMs)),
        ipcMs: stats(subset.map((operation) => operation.ipcMs))
      };
    };
    results.scenario = {
      load: "alternating scan_folder (20,000 new files) and remove_scan_root",
      scans: scanPhases.length,
      scanMs: stats(scanPhases.map((phase) => phase.endedAt - phase.startedAt)),
      removeMs: stats(load.phases.filter((phase) => phase.type === "remove").map((phase) => phase.endedAt - phase.startedAt)),
      indexedPerScan: stats(scanPhases.map((phase) => phase.indexed)),
      undeliveredInputs: operations.filter((operation) => !operation.inputDelivered).length,
      tag: summarize("tag"),
      favorite: summarize("favorite"),
      lightboxReadyMs: stats(bursts.map((burst) => burst.lightboxReadyMs)),
      startAssetQueryDuringScans: trace.calls.filter((call) => call.command === "start_asset_query" && inScan(call)).length,
      startAssetQueryTotal: trace.calls.filter((call) => call.command === "start_asset_query").length,
      stalePagesDuringScans: trace.calls.filter((call) => call.command === "get_asset_query_page" && call.result === "stale" && inScan(call)).length,
      phases: load.phases,
      ipcCounts: trace.counts,
      operations
    };
    console.log(`[grid-baseline] scenario ${JSON.stringify({ ...results.scenario, operations: undefined, ipcCounts: undefined, phases: undefined })}`);
  });

  it("measures fling settle, frames and DOM size with warm thumbnails", async () => {
    await browser.refresh();
    await waitForGallery();
    const rendered = Date.now();
    const renderSummary = await invokeLong("render_all_thumbnails");
    results.warm.renderAllMs = Date.now() - rendered;
    results.warm.renderSummary = renderSummary;
    await browser.refresh();
    await waitForGallery();
    const settle = await browser.executeAsync(flingAndSettle, { velocity: 0, decay: 1, direction: 1, timeoutMs: 30000 });
    results.warm.environment = await browser.execute(() => ({
      width: innerWidth,
      height: innerHeight,
      devicePixelRatio,
      total: document.querySelector('[data-testid="gallery-grid"]').closest(".gallery-scroll").scrollHeight,
      supportedEntryTypes: PerformanceObserver.supportedEntryTypes ?? [],
      documentNodes: document.getElementsByTagName("*").length
    }));
    results.warm.restingDom = { mountedTiles: settle.mountedTiles, gridNodes: settle.gridNodes, visible: settle.complete };
    const flings = [
      { name: "slow-down", velocity: 6, decay: 0.95, direction: 1 },
      { name: "medium-down", velocity: 20, decay: 0.95, direction: 1 },
      { name: "fast-down", velocity: 45, decay: 0.96, direction: 1 },
      { name: "fast-up", velocity: 45, decay: 0.96, direction: -1 },
      { name: "jump-80", jumpTo: 0.8, velocity: 0 },
      { name: "jump-30", jumpTo: 0.3, velocity: 0 },
      { name: "medium-down-2", velocity: 20, decay: 0.95, direction: 1 },
      { name: "fast-down-2", velocity: 45, decay: 0.96, direction: 1 }
    ];
    results.warm.flings = [];
    for (const spec of flings) {
      const sample = await browser.executeAsync(flingAndSettle, { decay: 1, direction: 1, ...spec, timeoutMs: 15000 });
      results.warm.flings.push(sample);
      await browser.pause(300);
    }
    const warmTrace = await browser.execute(() => window.gridTrace);
    const ipcMs = (command) => stats(warmTrace.calls
      .filter((call) => call.command === command && call.t1 !== null)
      .map((call) => call.t1 - call.t0));
    results.warm.summary = {
      pageIpcMs: ipcMs("get_asset_query_page"),
      startIpcMs: ipcMs("start_asset_query"),
      settleMs: stats(results.warm.flings.map((sample) => sample.settleMs)),
      maxFrameGapAfterStopMs: stats(results.warm.flings.map((sample) => sample.maxFrameGapAfterStop)),
      mountedTiles: stats(results.warm.flings.map((sample) => sample.mountedTiles)),
      gridNodes: stats(results.warm.flings.map((sample) => sample.gridNodes)),
      incomplete: results.warm.flings.filter((sample) => sample.settleMs === null).length
    };
    await fs.mkdir(output, { recursive: true });
    await browser.saveScreenshot(path.join(output, "warm-after-flings.png"));
    console.log(`[grid-baseline] warm ${JSON.stringify({ renderAllMs: results.warm.renderAllMs, ...results.warm.summary, restingDom: results.warm.restingDom })}`);
  });

  it("measures viewport-to-thumbnail time with an empty thumbnail cache", async () => {
    await invokeLong("clear_all_thumbnails");
    await browser.refresh();
    await waitForGallery();
    await fs.mkdir(output, { recursive: true });
    results.cold.jumps = [];
    for (const fraction of [0.15, 0.35, 0.55, 0.75, 0.95, 0.05]) {
      results.cold.jumps.push(await browser.executeAsync(coldJump, fraction, 120000));
      await browser.pause(300);
    }
    results.cold.summary = {
      perTileMs: stats(results.cold.jumps.flatMap((jump) => jump.perTileMs)),
      allVisibleMs: stats(results.cold.jumps.map((jump) => jump.allThumbnailsMs)),
      tilesMs: stats(results.cold.jumps.map((jump) => jump.tilesMs)),
      notReady: results.cold.jumps.filter((jump) => !jump.ready).length
    };
    await browser.saveScreenshot(path.join(output, "cold-last-jump.png"));
    console.log(`[grid-baseline] cold ${JSON.stringify(results.cold.summary)}`);
  });
});
