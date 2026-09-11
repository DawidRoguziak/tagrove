import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_ANIMATIONS === "1" ? describe : describe.skip;
const reduced = process.env.MEDIATAGGER_REDUCED_MOTION === "1";
const output = path.resolve("artifacts/animations", new Date().toISOString().replace(/[:.]/g, "-"));
let root;
async function click(selector) {
  if (selector === 'button[aria-label="Open asset panel"]') {
    // Keyboard activity reveals the existing idle trigger. Repeated WebDriver
    // moves to the same coordinates do not produce a mousemove in WebKit.
    await browser.keys("Tab");
    await browser.waitUntil(() => browser.execute(() =>
      getComputedStyle(document.getElementById("lightbox-sidebar-trigger-button")).opacity === "1"
    ));
  }
  await (await $(selector)).click();
}

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}

// Record before the WebDriver action, every rendering frame during it, and after it.
// Nothing here moves focus, scrolls the app, or changes layout.
async function sample(action) {
  await browser.execute(() => {
    const probe = { frames: [], pendingBulkFrames: [], entrances: [], running: true };
    window.__motionProbe = probe;
    probe.onStart = event => {
      if (event.animationName === "motion-fade-in") probe.entrances.push(event.target.className);
    };
    document.addEventListener("animationstart", probe.onStart);
    const record = () => {
      const frame = {};
      for (const selector of ["html", "body", "main", ".gallery-scroll", ".lightbox-layout", "[data-lightbox-toolbar]", '[data-testid="lightbox-sidebar-upper"]', '[role="listbox"]']) {
        const el = document.querySelector(selector);
        if (!el) continue;
        // Worker latency can expose a hidden, empty list for a frame in either
        // run. Compare visible results; surrounding scroll geometry is always sampled.
        if (selector === '[role="listbox"]' && getComputedStyle(el).visibility === "hidden") continue;
        frame[selector] = { width: el.clientWidth, height: el.clientHeight,
          scrollWidth: el.scrollWidth, scrollHeight: el.scrollHeight, left: el.scrollLeft, top: el.scrollTop };
        if (selector === ".lightbox-layout") {
          // overflow:clip is not a scroll container. WebKit caches the drawer's
          // transformed overflow here inconsistently, even with identical motion.
          // Check its viewport/position and clipping, and measure scroll extents
          // on the actual scroll containers surrounding it instead.
          delete frame[selector].scrollWidth;
          delete frame[selector].scrollHeight;
        }
        if (selector === '[role="listbox"]') {
          const rect = el.getBoundingClientRect();
          frame[selector].outside = Math.max(0, -rect.left, -rect.top, rect.right - innerWidth, rect.bottom - innerHeight);
        }
      }
      if (document.querySelector('.bulk-inspector [role="status"]')) probe.pendingBulkFrames.push(probe.frames.length);
      probe.frames.push(frame);
      if (probe.running) requestAnimationFrame(record);
    };
    record();
  });
  await action();
  await browser.pause(280);
  return browser.execute(() => {
    const probe = window.__motionProbe;
    probe.running = false;
    document.removeEventListener("animationstart", probe.onStart);
    return { frames: probe.frames, pendingBulkFrames: probe.pendingBulkFrames, entrances: probe.entrances };
  });
}

function stable(result, label) {
  assert.ok(result.frames.length > 3, `${label}: must sample rendering frames`);
  const before = result.frames[0];
  for (const frame of result.frames) {
    for (const selector of ["html", "body", "main", ".gallery-scroll", ".lightbox-layout"]) {
      if (before[selector] && frame[selector]) assert.deepEqual(frame[selector], before[selector], `${label}: ${selector}`);
    }
    if (frame['[role="listbox"]']) assert.ok(frame['[role="listbox"]'].outside <= 1, `${label}: suggestions stay inside viewport`);
  }
}

function compareLayout(actual, baseline, label) {
  assert.deepEqual(actual.frames.at(-1), baseline.frames.at(-1), `${label}: final geometry matches disabled motion`);
  for (const [index, frame] of actual.frames.entries()) {
    for (const [selector, geometry] of Object.entries(frame)) {
      const reference = baseline.frames.flatMap(f => f[selector] ? [f[selector]] : []);
      if (!reference.length) continue;
      for (const [key, value] of Object.entries(geometry)) {
        // The existing metadata loading row can occupy a single frame in only
        // one run. Its temporary height is functional; still compare all scroll
        // positions, viewport dimensions and the complete settled geometry.
        if (selector === ".gallery-scroll" && key === "scrollHeight" && actual.pendingBulkFrames.includes(index)) continue;
        const values = reference.map(g => g[key]);
        assert.ok(value >= Math.min(...values) - 1 && value <= Math.max(...values) + 1,
          `${label}: ${selector}.${key}=${value} outside disabled-motion range ${Math.min(...values)}..${Math.max(...values)}`);
      }
    }
  }
}

async function bottom() {
  await browser.waitUntil(() => browser.execute(() => {
    const el = document.querySelector(".gallery-scroll");
    el.scrollTop = el.scrollHeight;
    return Boolean(document.querySelector('button[data-asset-index="191"]'));
  }), { timeout: 20000, timeoutMsg: "Wait for gallery resize anchoring and the final page before measuring" });
  await $('button[data-asset-index="191"]').waitForDisplayed({ timeout: 20000 });
  // Below 1000px the inspector follows the gallery in the same scroll container.
  // Bring the last tile into view before sampling; WebDriver would otherwise scroll
  // from the inspector to the tile as part of its click.
  await browser.execute(() => document.querySelector('button[data-asset-index="191"]').scrollIntoView({ block: "end" }));
  await browser.pause(300);
}

async function reset(theme, disabled) {
  await browser.execute(value => {
    localStorage.setItem("media-tagger.theme", value);
    localStorage.setItem("media-tagger.language", "en");
  }, theme);
  await browser.refresh();
  await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
  await browser.execute(off => {
    if (off) {
      const style = document.createElement("style");
      // Retain the existing clipped drawer slide in both runs. WebKit includes its
      // transformed extent in the clipped layout's scrollWidth, even though that
      // element cannot scroll. This comparison isolates the new decorative motion.
      style.textContent = "*:not([data-lightbox-toolbar]), *::before, *::after { animation: none !important; transition: none !important; }";
      document.head.append(style);
    }
  }, disabled);
  // Warm lazy imports equally for the paired runs. No test-only production hooks.
  await click('button[aria-label="Enable bulk actions"]');
  await $('[data-testid="bulk-action-panel"]').waitForExist();
  await click('button[aria-label="Disable bulk actions"]');
  await bottom();
  if (!disabled) {
    const feedback = await browser.execute(() => {
      const control = getComputedStyle(document.getElementById("open-settings-button"));
      const spinner = document.createElement("span");
      spinner.className = "animate-spin";
      document.body.append(spinner);
      const loading = getComputedStyle(spinner).animationIterationCount;
      spinner.remove();
      return { duration: control.transitionDuration, properties: control.transitionProperty, loading };
    });
    assert.equal(feedback.duration, reduced ? "0s" : "0.1s");
    if (!reduced) assert.equal(feedback.properties, "background-color, border-color, color, box-shadow");
    assert.equal(feedback.loading, "infinite", "Reduced motion preserves loading indicators");
  }
}

async function exercise(theme, disabled) {
  await reset(theme, disabled);
  const results = {};
  const check = async (name, action, layout = false) => {
    const result = await sample(action);
    results[name] = result;
    if (!layout) stable(result, name);
    return result;
  };
  const tile = 'button[data-asset-index="191"]';
  await check("hover tile at gallery bottom", async () => (await $(tile)).moveTo());
  await check("button press", async () => {
    const point = await browser.execute(() => {
      const rect = document.querySelector('button[aria-label="Open tag list"]').getBoundingClientRect();
      return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
    });
    await browser.performActions([{ type: "pointer", id: "motion", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", duration: 0, ...point },
      { type: "pointerDown", button: 0 }, { type: "pause", duration: 180 }
    ] }]);
    const translate = await browser.execute(() => getComputedStyle(document.querySelector('button[aria-label="Open tag list"]')).translate);
    assert.ok(translate === "none" || translate === "0px", `Button press translated: ${translate}`);
    // Move away before releasing so the press does not also open the dialog.
    await browser.performActions([{ type: "pointer", id: "motion", parameters: { pointerType: "mouse" }, actions: [
      { type: "pointerMove", duration: 0, x: 2, y: 2 }, { type: "pointerUp", button: 0 }
    ] }]);
    await browser.releaseActions();
  });
  const suggestions = await check("suggestions", async () => {
    await $('.filter-input').setValue("motion");
    await $('[role="listbox"]').waitForDisplayed();
  });
  assert.equal(suggestions.entrances.length, disabled || reduced ? 0 : 1, "Suggestions enter once after positioning");
  const typing = await check("typing in suggestions", async () => $('.filter-input').addValue("-"));
  assert.equal(typing.entrances.length, 0, "Typing does not replay entrance");
  await check("dismiss suggestions", async () => browser.keys("Escape"));
  await browser.keys(["Control", "a"]);
  await browser.keys("Backspace");
  await browser.keys("Escape");
  const modal = await check("dialog open", async () => {
    await click('button[aria-label="Open tag list"]');
    await $('#tag-list-filter-input').waitForDisplayed();
  });
  assert.equal(modal.entrances.length, disabled || reduced ? 0 : 1, "Standard dialog fades");
  await check("dialog close", async () => browser.keys("Escape"));
  assert.equal(await $('[role="dialog"]').isExisting(), false);
  assert.equal(await $('button[aria-label="Open tag list"]').isFocused(), true);
  assert.equal(await browser.execute(() => document.getElementById("root").inert), false);
  await check("bulk open", async () => {
    await click('button[aria-label="Enable bulk actions"]');
    await $('[data-testid="bulk-action-panel"]').waitForExist();
  }, true);
  // Column changes legitimately preserve a gallery anchor. Re-establish the bottom
  // before checking selection so navigation and animation are measured separately.
  await bottom();
  for (const name of ["select bottom tile", "deselect bottom tile"]) {
    // Selection reveals existing bulk controls, which can increase content height
    // in the stacked layout even when all animation is disabled.
    const selection = await check(name, async () => {
      await click(tile);
      if (name === "select bottom tile") await $('#bulk-tag-draft-input').waitForEnabled();
    }, true);
    for (const frame of selection.frames) {
      assert.equal(frame[".gallery-scroll"].top, selection.frames[0][".gallery-scroll"].top, `${name}: scroll position`);
    }
  }
  await check("bulk close", async () => click('button[aria-label="Disable bulk actions"]'), true);
  await bottom();
  await check("lightbox open", async () => {
    await click(tile);
    await $('[data-lightbox-kind]').waitForExist();
  });
  assert.equal(await browser.execute(() => getComputedStyle(document.querySelector(".lightbox-layout")).overflow), "clip");
  if (await browser.execute(() => innerWidth >= 768)) {
    await click('button[aria-label="Close asset panel"]');
    await browser.pause(280);
  }
  for (let i = 0; i < 3; i++) {
    await check(`drawer open ${i}`, async () => click('button[aria-label="Open asset panel"]'), true);
    await check(`drawer close ${i}`, async () => click('button[aria-label="Close asset panel"]'), true);
  }
  await check("drawer open for info", async () => click('button[aria-label="Open asset panel"]'), true);
  await check("inline info", async () => click('button[aria-label="Show info"]'), true);
  await $('#lightbox-tag-draft-input').scrollIntoView();
  await browser.pause(280);
  await check("suggestions at drawer edge", async () => {
    await $('#lightbox-tag-draft-input').setValue("motion");
    await $('[role="listbox"]').waitForDisplayed();
  });
  await check("dismiss drawer suggestions", async () => browser.keys("Escape"));
  await check("inline confirmation", async () => click('#lightbox-delete-button'), true);
  await check("inline confirmation close", async () => browser.keys("Escape"), true);
  await check("lightbox close", async () => click('button[aria-label="Close preview"]'));
  return results;
}

suite("animation scroll geometry", function () {
  this.timeout(180000);
  before(async () => {
    await fs.mkdir(output, { recursive: true });
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-animations-"));
    await copyPngFixtures(root, "motion", 192);
    const longName = `${"long-filename-".repeat(12)}.png`;
    await fs.rename(path.join(root, "motion-1.png"), path.join(root, longName));
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await invoke("list_assets", { offset: 0, limit: 1, tagsAnd: [], tagsNot: [], kind: "all", favoritesOnly: false });
    await invoke("set_asset_tags", { assetId: page.items[0].id, tags: ["motion-example", `motion-${"long-label-".repeat(18)}`] });
    assert.equal(await browser.execute(() => matchMedia("(prefers-reduced-motion: reduce)").matches), reduced,
      "Launch with matching GTK gtk-enable-animations setting; do not emulate reduced motion with injected CSS");
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(output, "failure.png"));
      await fs.writeFile(path.join(output, "failure.json"), JSON.stringify(await browser.execute(() => ({
        text: document.body.innerText, frames: window.__motionProbe?.frames
      })), null, 2));
    }
    await browser.releaseActions();
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  for (const theme of ["dark", "light"]) {
    for (const width of [699, 700, 767, 768, 999, 1000]) {
      it(`${theme} at ${width}px, reduced motion ${reduced}`, async () => {
        await browser.setWindowSize(width, 720);
        assert.equal(await browser.execute(() => innerWidth), width, "Exercise the actual CSS breakpoint");
        const baseline = await exercise(theme, true);
        const animated = await exercise(theme, false);
        await fs.writeFile(path.join(output, `${theme}-${width}.json`), JSON.stringify({ baseline, animated }));
        for (const name of Object.keys(animated)) compareLayout(animated[name], baseline[name], name);
      });
    }
  }
});
