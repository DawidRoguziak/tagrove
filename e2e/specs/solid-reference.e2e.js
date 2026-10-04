import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

const suite = process.env.MEDIATAGGER_SOLID_REFERENCE === "1" ? describe : describe.skip;
const evidence = path.resolve("artifacts/solid2/stage1");
let root;

async function invoke(command, args = {}) {
  const result = await browser.executeAsync((command, args, done) => {
    window.__TAURI_INTERNALS__.invoke(command, args).then(
      value => done({ value }), error => done({ error: String(error) })
    );
  }, command, args);
  if (result.error) throw new Error(result.error);
  return result.value;
}

suite("React migration loading reference", () => {
  before(async () => {
    await fs.mkdir(evidence, { recursive: true });
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-solid-reference-"));
    await copyPngFixtures(root, "reference", 1);
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
  });

  after(async () => {
    await browser.execute(() => {
      window.releaseReferenceDetails?.();
      if (window.referenceFetch) window.fetch = window.referenceFetch;
    });
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });

  it("captures pending metadata and then shows the real response", async () => {
    await $('button[data-asset-index="0"]').waitForDisplayed();
    await browser.execute(() => {
      const original = window.fetch;
      window.referenceFetch = original;
      const gate = new Promise(resolve => { window.releaseReferenceDetails = resolve; });
      window.fetch = async function(input, options) {
        const response = await original.call(this, input, options);
        const url = new URL(String(input));
        if (url.protocol === "ipc:" && decodeURIComponent(url.pathname.slice(1)) === "get_asset_details") {
          await gate;
        }
        return response;
      };
    });
    await $('button[data-asset-index="0"]').click();
    await $('[data-lightbox-kind]').waitForDisplayed();
    if (!await $('#lightbox-sidebar[aria-hidden="false"]').isDisplayed()) {
      await $('#lightbox-sidebar-trigger-button').click();
    }
    const loading = $('[data-testid="lightbox-tag-panel"] [role="status"]');
    await loading.waitForDisplayed();
    assert.equal(await $('#lightbox-tag-draft-input').isEnabled(), false);
    await browser.saveScreenshot(path.join(evidence, "react-metadata-loading.png"));
    await browser.execute(() => window.releaseReferenceDetails());
    await $('#lightbox-tag-draft-input').waitForEnabled();
    await loading.waitForExist({ reverse: true });
    await browser.saveScreenshot(path.join(evidence, "react-metadata-ready.png"));
  });
});
