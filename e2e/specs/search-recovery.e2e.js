import { execFile } from "node:child_process";
import { promisify } from "node:util";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
import { listAssetsForAssertions } from "../assets.js";

const run = promisify(execFile);
const enabled = process.env.MEDIATAGGER_SEARCH_RECOVERY === "1";
const evidence = path.resolve("artifacts/search-recovery");
async function invoke(command, args = {}) {
  const response = await browser.executeAsync((command, args, done) => {
    window.__TAURI_INTERNALS__.invoke(command, args)
      .then(value => done({ value }), error => done({ error: String(error) }));
  }, command, args);
  if (response.error) throw new Error(response.error);
  return response.value;
}

async function nativeClick(selector, action = "click") {
  const point = await browser.execute(selector => {
    const bounds = document.querySelector(selector).getBoundingClientRect();
    return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
  }, selector);
  const rect = await browser.getWindowRect();
  await run("python3", ["e2e/native-input.py", action,
    String(Math.round(rect.x + point.x)), String(Math.round(rect.y + point.y))]);
}

(enabled ? describe : describe.skip)("Search and tag recovery", () => {
  let root;
  let untaggedId;
  before(async () => {
    await invoke("clear_library_data");
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-search-recovery-"));
    await copyPngFixtures(root, "recovery", 2);
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await listAssetsForAssertions({
      offset: 0, limit: 10, tagsAnd: [], tagsNot: [], kind: null, favoritesOnly: false, metaFilter: null
    });
    await invoke("set_asset_tags", { assetId: page.items[0].id, tags: ["cat", "constructor", "__proto__"] });
    untaggedId = page.items[1].id;
    await fs.mkdir(evidence, { recursive: true });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(path.join(evidence, "failure.png"));
      console.log("RECOVERY FAILURE", await browser.execute(() => ({
        text: document.body.innerText,
        inputs: [...document.querySelectorAll("input")].map(input => ({id: input.id, value: input.value, focused: input === document.activeElement})),
        faultInjected: window.__suggestionFaultInjected
      })));
    }
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });

  for (const editor of ["search", "lightbox", "bulk"]) {
    for (const method of ["pointer", "keyboard"]) {
      it(`${editor} retries through ${method} focus transfer and renders worker results`, async () => {
        await browser.refresh();
        await browser.setWindowSize(1440, 1000);
        await $(".filter-input").waitForDisplayed();
        // Fault only the first worker construction. Retry runs the real bundled worker.
        await browser.execute(() => {
          const OriginalWorker = window.Worker;
          window.Worker = class extends OriginalWorker {
            constructor() {
              window.__suggestionFaultInjected = true;
              window.Worker = OriginalWorker;
              throw new Error("Injected suggestion worker startup failure");
            }
          };
        });
        let selector = ".filter-input";
        if (editor !== "search") {
          if (editor === "bulk") {
            await $('button[aria-label="Enable bulk actions"]').click();
            selector = "#bulk-tag-draft-input";
          } else selector = "#lightbox-tag-draft-input";
          const tile = await $(`button[data-asset-id="${untaggedId}"]`);
          await tile.waitForDisplayed();
          await tile.click();
        }
        const input = await $(selector);
        await input.waitForDisplayed();
        await nativeClick(selector, "focus");
        await browser.waitUntil(() => browser.execute(selector =>
          document.hasFocus() && document.activeElement === document.querySelector(selector), selector));
        await browser.keys("c");
        await browser.keys("a");
        const retry = await $("button=Suggestions unavailable. Retry");
        await retry.waitForDisplayed();
        await expect(await browser.execute(() => window.__suggestionFaultInjected)).toBe(true);
        if (method === "pointer") {
          const retrySelector = `${selector} + button`;
          await nativeClick(retrySelector);
        } else {
          await browser.keys("Tab");
          await expect(retry).toBeFocused();
          await browser.keys("Enter");
        }
        const option = await $('[role="option"]');
        await option.waitForDisplayed();
        await expect(option).toHaveText("cat");
        await expect(input).toBeFocused();
        await browser.saveScreenshot(path.join(evidence, `${editor}-${method}.png`));
        await browser.keys("Escape");
        await expect(input).toHaveValue("ca");
      });
    }
  }

  for (const tag of ["constructor", "__proto__"]) {
    for (const action of ["single", "double", "keyboard"]) {
      it(`applies literal ${tag} with ${action} selection`, async () => {
        await browser.refresh();
        await $('button[aria-label="Open tag list"]').waitForDisplayed();
        await $('button[aria-label="Open tag list"]').click();
        const chip = await $(`button[aria-label="${tag}, state: inactive"]`);
        await chip.waitForDisplayed();
        if (action === "double") await chip.doubleClick();
        else if (action === "keyboard") {
          // Native Tab traversal from the filter to this chip.
          for (let step = 0; step < 6 && !(await chip.isFocused()); step++) await browser.keys("Tab");
          await expect(chip).toBeFocused();
          await browser.keys(["Shift", "Enter"]);
        } else await chip.click();
        await $("button=Apply").click();
        await expect($(".filter-input")).toHaveValue(`${action === "single" ? "" : "-"}${tag}`);
        await expect($(".workspace-count")).toHaveText("1 item");
      });
    }
  }
});
