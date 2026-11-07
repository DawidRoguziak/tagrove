import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
import { e2eWindowTitle } from "../build-e2e.js";
import { listAssetsForAssertions } from "../assets.js";

const suite = process.env.MEDIATAGGER_SETTINGS_MAINTENANCE === "1" ? describe : describe.skip;
const evidence = path.resolve("artifacts/settings-maintenance");
async function invoke(command, args = {}) {
  const result = await browser.executeAsync(
    (name, payload, done) => {
      window.__TAURI_INTERNALS__.invoke(name, payload).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) })
      );
    },
    command,
    args
  );
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function idle() {
  await browser.waitUntil(
    async () => !(await $('[data-testid="scan-section-loader"]').isExisting()),
    { timeout: 20000 }
  );
}
async function settings() {
  await $("#open-settings-button").waitForDisplayed();
  await $("#open-settings-button").click();
  await $('[data-testid="scan-settings-section"]').waitForDisplayed();
  await idle();
}
async function navigate(section) {
  const selector = `.settings-nav a[href="#settings-${section}"]`;
  await browser.execute(
    (selector) =>
      document.querySelector(selector).scrollIntoView({ block: "center", behavior: "instant" }),
    selector
  );
  await $(selector).click();
}
async function dialog() {
  return $('[role="dialog"][aria-labelledby="duplicate-resolver-heading"]');
}

suite("settings maintenance ownership and duplicate batches", () => {
  let fixture;
  before(async () => {
    await fs.mkdir(evidence, { recursive: true });
    if (process.env.MEDIATAGGER_REDUCED_MOTION === "1")
      assert.equal(
        await browser.execute(() => matchMedia("(prefers-reduced-motion: reduce)").matches),
        true
      );
  });
  beforeEach(async () => {
    await $(".filter-input").waitForDisplayed();
    assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
    await invoke("clear_library_data");
    fixture = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-settings-maintenance-"));
    for (const name of ["a", "b"]) {
      const root = path.join(fixture, name);
      await fs.mkdir(root);
      await copyPngFixtures(root, "same", 1);
      await invoke("scan_folder", { path: root });
    }
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await browser.execute(() => {
      window.__settingsErrors = [];
      const originalError = console.error;
      console.error = (...args) => {
        window.__settingsErrors.push(args.map((arg) => String(arg?.stack ?? arg)).join(" "));
        originalError(...args);
      };
      window.addEventListener("error", (event) =>
        window.__settingsErrors.push(String(event.error?.stack ?? event.message))
      );
      window.addEventListener("unhandledrejection", (event) =>
        window.__settingsErrors.push(String(event.reason?.stack ?? event.reason))
      );
    });
    await settings();
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await browser.saveScreenshot(
        path.join(evidence, `failed-${this.currentTest.title.replace(/[^a-z0-9]/gi, "-")}.png`)
      );
      console.log(
        "SETTINGS_FAILURE",
        await browser.execute(() => ({
          errors: window.__settingsErrors,
          dialogs: [...document.querySelectorAll('[role="dialog"]')].map((el) => el.textContent),
          pageError: document.querySelector("main [role=alert]")?.textContent
        }))
      );
    }
    await browser.execute(() => {
      window.__settingsRelease?.();
      window.__settingsRestoreFetch?.();
    });
    assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
    await invoke("clear_library_data");
    if (fixture) await fs.rm(fixture, { recursive: true, force: true });
    await browser.refresh();
    await browser.setWindowSize(1000, 800);
  });
  it("keeps a running scan and its lock across Settings unmount, while preferences remain usable", async () => {
    // Delay only delivery of the real backend result. The actual scan and events still run.
    await browser.execute(() => {
      const original = window.fetch;
      window.__settingsRestoreFetch = () => {
        window.fetch = original;
      };
      const gate = new Promise((resolve) => {
        window.__settingsRelease = resolve;
      });
      window.fetch = async (resource, options) => {
        const response = await original(resource, options);
        if (decodeURIComponent(String(resource)).includes("rescan_all_roots")) {
          window.__settingsGateReached = true;
          await gate;
        }
        return response;
      };
    });
    await $("button=Rescan all").click();
    await browser.waitUntil(() => browser.execute(() => window.__settingsGateReached === true));
    assert.equal(await $("button=Choose folder").isEnabled(), false);
    await $('button[aria-label="Back"]').click();
    await $("#open-settings-button").click();
    await $('[data-testid="scan-section-loader"]').waitForDisplayed();
    assert.equal(await $("button=Choose folder").isEnabled(), false);
    await navigate("appearance");
    assert.equal(await $('[type="radio"][value="dark"]').isEnabled(), true);
    await $('label.theme-choice:has(input[value="dark"])').click();
    assert.equal(await $("html").getAttribute("data-theme"), "dark");
    await browser.execute(() => window.__settingsRelease());
    await idle();
    assert.equal(await $("button=Choose folder").isEnabled(), true);
    assert.equal((await invoke("list_scan_roots")).length, 2);
    await browser.saveScreenshot(path.join(evidence, "scan-after-remount.png"));
  });
  for (const [theme, width] of [
    ["light", 1000],
    ["dark", 600]
  ]) {
    it(`stages rename and deletion, cancels confirmation, then commits one batch (${theme}, ${width}px)`, async () => {
      await browser.setWindowSize(width, 800);
      await navigate("appearance");
      await $(`label.theme-choice:has(input[value="${theme}"])`).click();
      await navigate("duplicates");
      await $("button=Start").click();
      await (await dialog()).waitForDisplayed();
      await browser.waitUntil(
        async () => (await $$('[role="dialog"] input[data-asset-id]')).length === 2
      );
      const fields = await $$('[role="dialog"] input[data-asset-id]');
      const renamedId = Number(await fields[0].getAttribute("data-asset-id"));
      const deletedId = Number(await fields[1].getAttribute("data-asset-id"));
      const before = await invoke("find_duplicate_assets");
      const renamed = before.groups[0].assets.find((asset) => asset.id === renamedId);
      const deleted = before.groups[0].assets.find((asset) => asset.id === deletedId);
      await fields[0].setValue("renamed.png");
      await $(`button[data-asset-id="${renamedId}"]*=Queue rename`).click();
      await $(`button[data-asset-id="${deletedId}"]*=Queue delete`).click();
      await $("button=Save all").click();
      const confirm = $('[role="dialog"][aria-labelledby="duplicate-delete-confirm-heading"]');
      await confirm.waitForDisplayed();
      await browser.saveScreenshot(path.join(evidence, `confirm-${theme}-${width}.png`));
      await confirm.$("button=Cancel").click();
      await confirm.waitForDisplayed({ reverse: true });
      await fs.access(renamed.path);
      await fs.access(deleted.path);
      await $("button=Save all").click();
      await confirm.waitForDisplayed();
      await confirm.$("button=Confirm").click();
      await browser.waitUntil(
        async () => (await invoke("find_duplicate_assets")).duplicate_groups === 0
      );
      await fs.access(path.join(path.dirname(renamed.path), "renamed.png"));
      await assert.rejects(fs.access(renamed.path));
      await assert.rejects(fs.access(deleted.path));
      await browser.waitUntil(async () =>
        (await dialog()).getText().then((text) => text.includes("No duplicate groups"))
      );
      await confirm.waitForExist({ reverse: true });
      await browser.saveScreenshot(path.join(evidence, `resolved-${theme}-${width}.png`));
      await (await dialog()).$("button=Close").click();
      await $('button[aria-label="Back"]').click();
      await browser.refresh();
      const assets = await listAssetsForAssertions({
        offset: 0,
        limit: 10,
        tagsAnd: [],
        tagsNot: [],
        kind: null,
        favoritesOnly: false
      });
      assert.equal(assets.total, 1);
      assert.equal(assets.items[0].path, path.join(path.dirname(renamed.path), "renamed.png"));
    });
  }
});
