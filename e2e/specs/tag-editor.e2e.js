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

async function monitorTagIpc() {
  await browser.execute(() => {
    // The invoke property is immutable; observe custom-protocol fetches and cloned responses.
    const original = window.fetch;
    window.restoreTagFetch = () => { window.fetch = original; delete window.restoreTagFetch; };
    window.tagIpc = { pages: 0, pendingPages: 0, saves: [] };
    window.fetch = async function (input, options) {
      const url = new URL(input instanceof Request ? input.url : String(input), location.href);
      const command = url.protocol === "ipc:" ? decodeURIComponent(url.pathname.slice(1)) : "";
      const page = command === "list_tags";
      if (page) { window.tagIpc.pages++; window.tagIpc.pendingPages++; }
      try {
        const response = await original.call(this, input, options);
        if (["set_asset_tags", "merge_asset_tags_bulk"].includes(command)) {
          window.tagIpc.saves.push(await response.clone().json());
        }
        return response;
      } finally {
        if (page) window.tagIpc.pendingPages--;
      }
    };
  });
}

async function ipcSnapshot() {
  await browser.waitUntil(() => browser.execute(() => window.tagIpc.pendingPages === 0));
  return browser.execute(() => window.tagIpc);
}

async function assertVocabularySave(before, vocabularyChanged) {
  await browser.waitUntil(() => browser.execute(count => window.tagIpc.saves.length > count, before.saves.length));
  // The cloned IPC response arrives before React handles the save. Wait for the
  // editor's completion state, which is published after requesting any refresh.
  await browser.waitUntil(() => browser.execute(() => {
    const input = document.querySelector('[data-ui="tag-editor"] input');
    const panel = document.querySelector('[data-testid="lightbox-tag-panel"]');
    return input && !input.disabled && input.value === "" &&
      !panel?.textContent.includes("Saving tags...");
  }), { timeoutMsg: "Tag editor did not finish saving" });
  if (vocabularyChanged) {
    await browser.waitUntil(() => browser.execute(pages => window.tagIpc.pages > pages, before.pages));
  }
  const after = await ipcSnapshot();
  assert.equal(after.saves.length, before.saves.length + 1, "One editor action should commit one save");
  assert.equal(after.saves.at(-1).vocabulary_changed, vocabularyChanged);
  assert.equal(after.pages - before.pages, vocabularyChanged ? 1 : 0,
    "Small vocabulary should fetch exactly one page only when names change");
  return after;
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
    // Earlier specs share the profile; the exact-count assertion below needs an empty library.
    await invoke("clear_library_data");
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
    await browser.execute(() => window.restoreTagFetch?.());
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(evidence, "failure.png"));
      await fs.writeFile(path.join(evidence, "failure.txt"), await $("body").getText());
    }
  });
  after(async () => {
    await invoke("clear_library_data");
    if (mediaRoot) {
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
          // Crossing the breakpoint would collapse the lightbox drawer.
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

  for (const kinds of [["lightbox", "bulk"], ["bulk"]]) {
    it(`persists ${kinds.join(" and ")} writes and places suggestions outside the scroll field`, async () => {
      for (const assetId of assetIds) {
        const details = await invoke("get_asset_details", { assetId });
        await invoke("set_asset_tags", { assetId, tags: details.tags.filter(tag => tag !== "multi-proof" && (assetId !== assetIds[0] || tag !== "suggestion-only")) });
      }
      await browser.refresh();
      await browser.setWindowSize(1100, 1000);
      await $("button[data-asset-id]").waitForDisplayed();
      for (const kind of kinds) {
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
          const field = input.getBoundingClientRect();
          return { portal: list.parentElement === document.body, top: r.top, bottom: r.bottom,
            inputTop: field.top, inputBottom: field.bottom, viewport: innerHeight };
        }, selector);
        assert.ok(popup.portal, `${kind} suggestions are not portalled to <body>`);
        // The bulk dock sits at the bottom of the viewport, so it opens suggestions
        // above the input; the lightbox editor opens them below.
        if (kind === "bulk") {
          assert.ok(popup.bottom <= popup.inputTop && popup.top >= 0, `bulk suggestions are not above the input: ${JSON.stringify(popup)}`);
        } else {
          assert.ok(popup.top >= popup.inputBottom && popup.bottom <= popup.viewport, `${kind} suggestions are not below the input: ${JSON.stringify(popup)}`);
        }
        // Suggestions are published only after the complete startup vocabulary has
        // loaded. Start counting after that barrier, not merely after gallery load.
        if (kind === kinds[0]) await monitorTagIpc();
        let before = await ipcSnapshot();
        await browser.keys(["ArrowDown", "Enter"]);
        await browser.waitUntil(async () => (await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes("suggestion-only"));
        before = await assertVocabularySave(before, false);
        await $('button[aria-label="Remove tag suggestion-only"]').click();
        await browser.waitUntil(async () => !(await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes("suggestion-only"));
        before = await assertVocabularySave(before, false);
        for (let repeat = 0; repeat < 2; repeat++) {
          await $(selector).setValue("suggestion-only");
          await browser.keys("Enter");
          before = await assertVocabularySave(before, false);
          await $('button[aria-label="Remove tag suggestion-only"]').click();
          before = await assertVocabularySave(before, false);
        }
        const newTag = `vocabulary-proof-${kind}`;
        await $(selector).setValue(newTag);
        await browser.keys("Enter");
        before = await assertVocabularySave(before, true);
        assert.ok((await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes(newTag));
        await $(`button[aria-label="Remove tag ${newTag}"]`).click();
        await assertVocabularySave(before, true);
        assert.ok(!(await invoke("get_asset_details", { assetId: assetIds[0] })).tags.includes(newTag));
        if (kind === "lightbox") await $('button[aria-label="Close preview"]').click();
      }
      await $(`button[data-asset-id="${assetIds[1]}"]`).click();
      let before = await ipcSnapshot();
      await $("#bulk-tag-draft-input").setValue("suggestion-only");
      await browser.keys("Enter");
      before = await assertVocabularySave(before, false);
      await $("#bulk-tag-draft-input").setValue("multi-proof");
      await browser.keys("Enter");
      await browser.waitUntil(async () => (await Promise.all(assetIds.map(assetId => invoke("get_asset_details", { assetId })))).every(asset => asset.tags.includes("multi-proof")));
      await assertVocabularySave(before, true);
      await fs.writeFile(path.join(evidence, "vocabulary-ipc.json"), JSON.stringify(await ipcSnapshot(), null, 2));
      assert.equal(await $('[data-ui="tag-editor"] button').isExisting(), false);
      await browser.execute(() => window.restoreTagFetch());
      await browser.refresh();
      await $("button[data-asset-id]").waitForDisplayed();
      for (const assetId of assetIds) {
        assert.ok((await invoke("get_asset_details", { assetId })).tags.includes("multi-proof"));
      }
      if (kinds.length === 1) await $('button[aria-label="Enable bulk actions"]').click();
      await $(`button[data-asset-id="${assetIds[0]}"]`).click();
      await browser.waitUntil(async () => (await $('[data-ui="tag-editor"]').getText()).includes("multi-proof"),
        { timeoutMsg: "Reloaded editor did not show persisted tags" });
    });
  }
});
