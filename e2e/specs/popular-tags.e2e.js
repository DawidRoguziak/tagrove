import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
import { e2eWindowTitle } from "../build-e2e.js";

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync(
    (name, args, done) => {
      window.__TAURI_INTERNALS__.invoke(name, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) })
      );
    },
    command,
    payload
  );
  if (result.error) throw new Error(result.error);
  return result.value;
}

async function ready() {
  await $(".filter-input").waitForDisplayed({ timeout: 20000 });
  assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
}

const group = '[role="group"][aria-label="Most used tags"]';
const chips = `${group} button`;
async function chipNames() {
  return browser.execute(
    (selector) => [...document.querySelectorAll(selector)].map((button) => button.textContent),
    chips
  );
}

async function selectFirst() {
  await $('button[aria-label="Enable bulk actions"]').click();
  await $('button[data-asset-index="0"]').waitForDisplayed();
  const id = Number(await $('button[data-asset-index="0"]').getAttribute("data-asset-id"));
  await $('button[data-asset-index="0"]').click();
  await $("#bulk-tag-draft-input").waitForEnabled();
  return id;
}

describe("startup popular tags", () => {
  let fixture;
  const output = path.resolve("artifacts/popular-tags");
  after(async () => {
    await ready();
    await invoke("clear_library_data");
    if (fixture) await fs.rm(fixture, { recursive: true, force: true });
  });

  it("applies chips and keeps the launch ranking until a fresh process", async () => {
    await ready();
    await invoke("clear_library_data");
    fixture = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-popular-tags-"));
    await copyPngFixtures(fixture, "popular", 3);
    await invoke("scan_folder", { path: fixture });
    const page = await invoke("list_assets", {
      offset: 0,
      limit: 100,
      tagsAnd: [],
      tagsNot: [],
      kind: null,
      favoritesOnly: false
    });
    const ids = page.items.map((asset) => asset.id);
    await invoke("set_asset_tags", { assetId: ids[0], tags: ["zebra", "alpha"] });
    await invoke("set_asset_tags", { assetId: ids[1], tags: ["zebra", "beta"] });
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    assert.deepEqual(await invoke("get_startup_popular_tags"), []);

    await browser.reloadSession();
    await ready();
    const expected = ["zebra", "alpha", "beta"];
    assert.deepEqual(await invoke("get_startup_popular_tags"), expected);
    const target = await selectFirst();
    assert.deepEqual(await chipNames(), expected);
    const beta = await $(group).$("button=beta");
    await beta.click();
    await browser.waitUntil(async () =>
      (await invoke("get_asset_details", { assetId: target })).tags.includes("beta")
    );
    await browser.waitUntil(() =>
      browser.execute(() => document.activeElement?.id === "bulk-tag-draft-input")
    );
    assert.deepEqual(await chipNames(), expected);
    await fs.mkdir(output, { recursive: true });
    await browser.saveScreenshot(path.join(output, "single-selection.png"));

    // A new tag becomes the most popular during this process.
    await invoke("merge_asset_tags_bulk", { assetIds: ids, tags: ["new-leader"] });
    await invoke("rescan_all_roots");
    assert.deepEqual(await invoke("get_startup_popular_tags"), expected);
    await $('button[aria-label="Disable bulk actions"]').click();
    await selectFirst();
    assert.deepEqual(await chipNames(), expected);
    await browser.refresh();
    await ready();
    await selectFirst();
    assert.deepEqual(await chipNames(), expected);
    await fs.writeFile(
      path.join(output, "persistence.json"),
      JSON.stringify(
        {
          startup: expected,
          afterReload: await invoke("get_startup_popular_tags"),
          target: await invoke("get_asset_details", { assetId: target })
        },
        null,
        2
      )
    );

    await browser.reloadSession();
    await ready();
    assert.equal((await invoke("get_startup_popular_tags"))[0], "new-leader");
    await invoke("clear_library_data");
    assert.equal((await invoke("get_startup_popular_tags"))[0], "new-leader");
    await browser.reloadSession();
    await ready();
    assert.deepEqual(await invoke("get_startup_popular_tags"), []);
  });
});
