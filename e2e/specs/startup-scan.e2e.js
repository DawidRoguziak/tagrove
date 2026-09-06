import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";
import { e2eWindowTitle } from "../build-e2e.js";

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(
      value => done({ value }), error => done({ error: String(error) })
    );
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}

async function ready() {
  await $(".filter-input").waitForDisplayed({ timeout: 20000 });
  assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
}

async function paths() {
  const page = await invoke("list_assets", {
    offset: 0, limit: 100, tagsAnd: [], tagsNot: [], kind: null, favoritesOnly: false
  });
  return page.items.map(asset => asset.path).sort();
}

async function openSettings() {
  await $('button[aria-label="Open settings"]').click();
  await $('[data-testid="scan-settings-section"]').waitForDisplayed();
}

async function waitForScanIdle() {
  await browser.waitUntil(async () => !(await $('[data-testid="scan-section-loader"]').isExisting()), {
    timeout: 20000, timeoutMsg: "Startup scan did not finish"
  });
}

describe("per-folder startup scans", () => {
  let fixture;
  after(async () => {
    // The suite verifies the isolated app title before any destructive IPC.
    assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
    await invoke("clear_library_data");
    if (fixture) await fs.rm(fixture, { recursive: true, force: true });
  });

  it("scans checked folders only on fresh launches and retains manual scanning", async () => {
    await ready();
    await invoke("clear_library_data");
    fixture = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-startup-scan-"));
    const enabled = path.join(fixture, "enabled");
    const disabled = path.join(fixture, "disabled");
    await fs.mkdir(enabled);
    await fs.mkdir(disabled);
    await copyPngFixtures(enabled, "enabled", 1);
    await copyPngFixtures(disabled, "disabled", 1);
    await invoke("add_scan_root", { path: enabled });
    await invoke("add_scan_root", { path: disabled });
    await browser.refresh();
    await ready();
    await openSettings();
    await waitForScanIdle();
    const checkbox = await $(`input[aria-label="Scan on app startup: ${enabled}"]`);
    assert.equal(await checkbox.isSelected(), false);
    await checkbox.click();
    await browser.waitUntil(async () => (await invoke("list_scan_roots"))
      .find(root => root.path === enabled)?.auto_scan_on_startup === true);
    assert.deepEqual(await paths(), []);

    // A WebView reload must not consume a new launch snapshot.
    await browser.refresh();
    await ready();
    await openSettings();
    await waitForScanIdle();
    assert.equal(await $(`input[aria-label="Scan on app startup: ${enabled}"]`).isSelected(), true);
    assert.deepEqual(await paths(), []);

    await browser.reloadSession();
    await ready();
    await browser.waitUntil(async () => (await paths()).length === 1, { timeout: 20000 });
    assert.deepEqual(await paths(), [path.join(enabled, "enabled-1.png")]);
    await openSettings();
    await waitForScanIdle();
    await fs.copyFile(path.join(enabled, "enabled-1.png"), path.join(enabled, "later.png"));
    await browser.refresh();
    await ready();
    await openSettings();
    await waitForScanIdle();
    assert.equal((await paths()).length, 1);
    assert.equal(await invoke("scan_startup_roots"), null);

    await browser.reloadSession();
    await ready();
    await browser.waitUntil(async () => (await paths()).length === 2, { timeout: 20000 });
    assert.deepEqual(await paths(), [path.join(enabled, "enabled-1.png"), path.join(enabled, "later.png")]);
    await openSettings();
    await waitForScanIdle();

    // Manual Rescan all still includes unchecked folders.
    await $('button=Rescan all').click();
    await browser.waitUntil(async () => (await paths()).length === 3, { timeout: 20000 });
    await waitForScanIdle();
    await $(`input[aria-label="Scan on app startup: ${enabled}"]`).click();
    await browser.waitUntil(async () => (await invoke("list_scan_roots"))
      .every(root => !root.auto_scan_on_startup));
    await fs.copyFile(path.join(enabled, "enabled-1.png"), path.join(enabled, "disabled-later.png"));
    await browser.reloadSession();
    await ready();
    await openSettings();
    await waitForScanIdle();
    assert.equal((await paths()).length, 3);
  });
});
