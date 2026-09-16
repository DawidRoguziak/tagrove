import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { copyPngFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_TAG_EDITOR === "1" ? describe : describe.skip;
const run = promisify(execFile);
const evidence = path.resolve("artifacts/tag-editor", new Date().toISOString().replaceAll(":", "-"));
let mediaRoot;
let assetIds;
const longTag = "long-synthetic-tag-".repeat(8);
const tags = [...Array.from({ length: 24 }, (_, index) => `fixture-${index}`), longTag];

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}

async function typeText(text) {
  // Separate key actions preserve repeated characters in WebKit's action endpoint.
  for (const character of text) await browser.keys(character);
}

async function openEditor(kind) {
  if (kind === "bulk") await $('button[aria-label="Enable bulk actions"]').click();
  await $(`button[data-asset-id="${assetIds[0]}"]`).click();
  if (kind === "lightbox") {
    await browser.keys("Tab"); // Wake the lightbox's inactivity controls.
    const open = await $('button[aria-label="Open asset panel"]');
    if (await open.isDisplayed()) await open.click();
  }
  await $(`#${kind}-tag-draft-input`).waitForExist();
}

async function nativeFocus(selector) {
  await browser.waitUntil(() => browser.executeAsync((selector, done) => {
    const field = document.querySelector(selector).closest('[data-ui="tag-editor"]');
    field.scrollIntoView({ block: "center" });
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const r = field.getBoundingClientRect();
      done(r.top >= 0 && r.bottom <= innerHeight);
    }));
  }, selector), { timeoutMsg: "Editor could not be scrolled into the viewport" });
  await browser.waitUntil(() => browser.execute(selector => {
    for (let node = document.querySelector(selector); node; node = node.parentElement) {
      if (node.getAnimations().some(animation => animation.playState === "running")) return false;
    }
    return true;
  }, selector));
  const point = await browser.execute(selector => {
    const r = document.querySelector(selector).closest('[data-ui="tag-editor"]').getBoundingClientRect();
    return { x: r.x + 3, y: r.y + r.height / 2 };
  }, selector);
  const window = await browser.getWindowRect();
  await run("python3", ["e2e/native-input.py", "focus",
    String(Math.round(window.x + point.x)), String(Math.round(window.y + point.y))]);
  await browser.waitUntil(() => browser.execute(selector =>
    document.hasFocus() && document.activeElement === document.querySelector(selector), selector));
}

async function geometry(kind) {
  return browser.execute(kind => {
    const input = document.querySelector(`#${kind}-tag-draft-input`);
    const field = input.closest('[data-ui="tag-editor"]');
    const rect = node => {
      const r = node.getBoundingClientRect();
      return { top: r.top, bottom: r.bottom, left: r.left, right: r.right, height: r.height };
    };
    const inputStyle = getComputedStyle(input);
    const fieldStyle = getComputedStyle(field);
    return {
      field: rect(field), input: rect(input), scrollTop: field.scrollTop,
      scrollHeight: field.scrollHeight, clientHeight: field.clientHeight,
      scrollWidth: field.scrollWidth, clientWidth: field.clientWidth,
      chips: [...field.children].filter(node => node.tagName === "SPAN").map(rect),
      focused: document.activeElement === input, matchesFocus: input.matches(":focus"),
      border: inputStyle.borderTopWidth, shadow: inputStyle.boxShadow,
      outline: inputStyle.outlineStyle, fieldShadow: fieldStyle.boxShadow,
      documentScroll: document.scrollingElement.scrollTop,
      outerScroll: (() => {
        const offsets = [];
        for (let node = field.parentElement; node; node = node.parentElement) offsets.push(node.scrollTop);
        return offsets;
      })()
    };
  }, kind);
}

function assertLayout(g, limit) {
  assert.ok(g.focused && g.matchesFocus, "Input must have real focus");
  assert.ok(g.field.height <= limit + 1, "Field exceeded its height limit");
  assert.ok(g.scrollHeight > g.clientHeight, "Fixture must exercise overflow");
  assert.ok(g.scrollTop > 0, "Focused input should reveal the end of the field");
  assert.ok(g.input.top >= g.field.top - 1 && g.input.bottom <= g.field.bottom + 1, "Input clipped by its own field");
  assert.ok(g.scrollWidth <= g.clientWidth + 1, "Long tags caused horizontal overflow");
  assert.ok(g.chips.every(chip => chip.left >= g.field.left && chip.right <= g.field.right), "Chip escaped field");
  assert.ok(g.chips.some(chip => chip.height > 40), "Long tag did not wrap");
  assert.equal(g.border, "0px");
  // Tailwind represents shadow-none as transparent zero-sized shadow layers.
  assert.ok(g.shadow === "none" ||
    g.shadow.replaceAll("rgba(0, 0, 0, 0) 0px 0px 0px 0px", "").replaceAll(", ", "") === "",
    "Input must not paint its own shadow");
  assert.equal(g.outline, "none");
  assert.notEqual(g.fieldShadow, "none", "Outer field must show focus feedback");
}

suite("shared inline tag editor", function () {
  this.timeout(180000);
  before(async () => {
    assert.ok(process.env.XDG_DATA_HOME?.startsWith("/tmp/mediatagger-"), "Requires disposable XDG profile");
    assert.equal(process.env.DISPLAY, process.env.MEDIATAGGER_NATIVE_DISPLAY);
    assert.ok(![":0", ":1"].includes(process.env.DISPLAY), "Requires a private display");
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-tag-editor-"));
    await fs.mkdir(evidence, { recursive: true });
    await copyPngFixtures(mediaRoot, "tag-editor", 2);
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $("button[data-asset-id]").waitForDisplayed({ timeout: 30000 });
    assetIds = await browser.execute(() => [...document.querySelectorAll("button[data-asset-id]")].map(node => Number(node.dataset.assetId)));
    assert.equal(assetIds.length, 2);
    // Fixture setup only. The persistence test below writes through the visible editors.
    await invoke("set_asset_tags", { assetId: assetIds[0], tags });
    await invoke("set_asset_tags", { assetId: assetIds[1], tags: ["suggestion-only"] });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(evidence, "failure.png"));
      await fs.writeFile(path.join(evidence, "failure.txt"), await $("body").getText());
    }
  });
  after(async () => {
    if (mediaRoot) {
      await invoke("remove_scan_root", { path: mediaRoot });
      await fs.rm(mediaRoot, { recursive: true, force: true });
    }
  });

  for (const kind of ["lightbox", "bulk"]) {
    for (const theme of ["light", "dark"]) {
      it(`keeps ${kind} chips and focused input visible in ${theme} at desktop and narrow widths`, async () => {
        await browser.execute(theme => localStorage.setItem("media-tagger.theme", theme), theme);
        await browser.refresh();
        await browser.setWindowSize(1100, 1000);
        await $("button[data-asset-id]").waitForDisplayed();
        await openEditor(kind);
        const selector = `#${kind}-tag-draft-input`;
        await nativeFocus(selector);
        for (const width of [1100, 600]) {
          await browser.setWindowSize(width, 1000);
          if (kind === "lightbox") {
            await browser.keys("Tab");
            const open = await $('button[aria-label="Open asset panel"]');
            if (await open.isDisplayed()) await open.click();
          }
          await nativeFocus(selector);
          const before = await geometry(kind);
          await typeText("new-draft");
          const after = await geometry(kind);
          assertLayout(after, kind === "bulk" ? 144 : 200);
          assert.equal(after.documentScroll, before.documentScroll, "Typing moved page scroll");
          assert.deepEqual(after.outerScroll, before.outerScroll, "Typing moved parent scroll");
          await fs.writeFile(path.join(evidence, `${kind}-${theme}-${width}.json`), JSON.stringify(after, null, 2));
          await browser.saveScreenshot(path.join(evidence, `${kind}-${theme}-${width}.png`));
          await browser.keys(["Control", "a"]);
          await browser.keys("Backspace");
          // Resize within the same responsive layout so the editor remains mounted.
          // Crossing the breakpoint remounts bulk or collapses the lightbox drawer.
          await browser.setWindowSize(width + 40, 1000);
          await browser.waitUntil(async () => {
            const g = await geometry(kind);
            return g.focused && g.input.top >= g.field.top - 1 && g.input.bottom <= g.field.bottom + 1;
          });
          assertLayout(await geometry(kind), kind === "bulk" ? 144 : 200);
          await typeText(`added-${kind}-${theme}-${width}`);
          const beforeAdd = await geometry(kind);
          await browser.keys("Enter");
          await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes(`added-${kind}-${theme}-${width}`));
          await browser.waitUntil(async () => (await geometry(kind)).focused);
          const afterAdd = await geometry(kind);
          assertLayout(afterAdd, kind === "bulk" ? 144 : 200);
          assert.equal(afterAdd.documentScroll, beforeAdd.documentScroll);
          assert.deepEqual(afterAdd.outerScroll, beforeAdd.outerScroll);
        }
      });
    }
  }

  it("persists lightbox and bulk writes and places suggestions outside the scroll field", async () => {
    await browser.refresh();
    await browser.setWindowSize(1100, 1000);
    await $("button[data-asset-id]").waitForDisplayed();
    for (const kind of ["lightbox", "bulk"]) {
      await openEditor(kind);
      const selector = `#${kind}-tag-draft-input`;
      await nativeFocus(selector);
      await $(selector).setValue("suggestion");
      await nativeFocus(selector);
      await $('[role="option"]').waitForDisplayed();
      const popup = await browser.execute(selector => {
        const list = document.querySelector('[role="listbox"]');
        const input = document.querySelector(selector);
        const r = list.getBoundingClientRect();
        return { portal: list.parentElement === document.body, top: r.top, bottom: r.bottom,
          inputBottom: input.getBoundingClientRect().bottom, viewport: innerHeight };
      }, selector);
      assert.ok(popup.portal && popup.top >= popup.inputBottom && popup.bottom <= popup.viewport);
      await browser.keys(["ArrowDown", "Enter"]);
      await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes("suggestion-only"));
      await $('button[aria-label="Remove tag suggestion-only"]').click();
      await browser.waitUntil(async () => !(await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes("suggestion-only"));
      if (kind === "lightbox") await $('button[aria-label="Close preview"]').click();
    }
    await $(`button[data-asset-id="${assetIds[1]}"]`).click();
    await $("#bulk-tag-draft-input").setValue("multi-proof");
    await browser.keys("Enter");
    await browser.waitUntil(async () => (await Promise.all(assetIds.map(assetId => invoke("get_asset_details", { assetId })))).every(asset => asset.tags.includes("multi-proof")));
    assert.equal(await $('[data-ui="tag-editor"] button').isExisting(), false);
    await browser.refresh();
    await $("button[data-asset-id]").waitForDisplayed();
    await $(`button[data-asset-id="${assetIds[0]}"]`).click();
    assert.ok((await $('[data-ui="tag-editor"]').getText()).includes("multi-proof"));
  });
});
