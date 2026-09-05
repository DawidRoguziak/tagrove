import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

const execFileAsync = promisify(execFile);
const suite = process.env.MEDIATAGGER_UI_REDESIGN === "1" ? describe : describe.skip;
const evidence = path.resolve("artifacts/ui-redesign", new Date().toISOString().replace(/[:.]/g, "-"));
let root;
let imageId;

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(
      (value) => done({ value }), (error) => done({ error: String(error) })
    );
  }, command, payload);
  if (result.error) throw new Error(result.error);
  return result.value;
}
async function click(selector) {
  const element = await $(selector);
  await element.waitForDisplayed({ timeout: 10000 });
  await element.click();
}
async function capture(name) {
  // Allow the app's short color/position transitions to settle before inspecting pixels.
  await browser.pause(250);
  const { x, y, width, height } = await browser.getWindowRect();
  // Capture only this window on the private display, including native GTK controls.
  await execFileAsync("import", ["-window", "root", "-crop", `${width}x${height}+${x}+${y}`, path.join(evidence, `${name}.png`)]);
}
async function galleryReady() {
  await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
  await browser.waitUntil(() => browser.execute(() => {
    const images = [...document.querySelectorAll('button[data-asset-id] img')];
    return images.length > 0 && images.every((img) => img.complete && img.naturalWidth > 1);
  }), { timeout: 20000 });
}
async function closeLightbox() {
  await click('button[aria-label="Close preview"]');
  await $('[data-lightbox-kind]').waitForExist({ reverse: true });
}

suite("UI redesign desktop review", function () {
  this.timeout(240000);
  before(async () => {
    await fs.mkdir(evidence, { recursive: true });
    console.log("UI_REDESIGN_EVIDENCE", evidence);
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-ui-redesign-"));
    await copyPngFixtures(root, "study", 24);
    await fs.mkdir(path.join(root, "duplicates"));
    await fs.copyFile(path.join(root, "study-1.png"), path.join(root, "duplicates", "study-1.png"));
    await createPlayableFixtures(root, "motion");
    // The harness has already verified the isolated E2E window identity.
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await invoke("list_assets", { offset: 0, limit: 200, tagsAnd: [], tagsNot: [], kind: "image", favoritesOnly: false });
    imageId = page.items[0].id;
    await invoke("set_asset_tags", { assetId: imageId, tags: ["architecture", "reference", "studio", "long_tag_".repeat(18)] });
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await galleryReady();
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });

  for (const theme of ["dark", "light"]) {
    for (const [width, height] of [[1440, 900], [1000, 720]]) {
      it(`keeps ${theme} controls usable at ${width}×${height}`, async () => {
        await browser.setWindowSize(width, height);
        await browser.refresh();
        await click('#open-settings-button');
        await $('#settings-theme-select').selectByAttribute("value", theme);
        await browser.waitUntil(async () => (await $('html').getAttribute('data-theme')) === theme);
        await capture(`${theme}-${width}-settings`);
        await click('.settings-nav a[href="#settings-appearance"]');
        assert.equal(await $('#settings-appearance').isFocused(), true);
        const sectionTop = await browser.execute(() => document.getElementById('settings-appearance').getBoundingClientRect().top);
        assert.ok(sectionTop >= 65 && sectionTop < height - 100, `Section top: ${sectionTop}`);
        await click('button[aria-label="Back"]');
        await galleryReady();
        assert.equal(await browser.execute(() => document.documentElement.scrollWidth <= innerWidth), true);
        const primary = await browser.execute(() => getComputedStyle(document.documentElement).getPropertyValue('--color-primary').trim());
        assert.equal(primary.toLowerCase(), theme === 'dark' ? '#7cb87c' : '#2d5a2d');
        await capture(`${theme}-${width}-gallery`);

        await $('.filter-input').setValue('arch');
        await $('[role="listbox"]').waitForDisplayed();
        await capture(`${theme}-${width}-suggestions`);
        await browser.keys('Escape');
        await $('.filter-input').setValue('absent-redesign-fixture');
        await click('button=Search');
        await browser.waitUntil(async () => (await $$('button[data-asset-id]')).length === 0);
        await capture(`${theme}-${width}-empty`);
        await click('button[aria-label="Clear all search filters"]');
        await galleryReady();
        await $('.filter-input').setValue('tags:invalid');
        await click('button=Search');
        await $('[role="alert"]').waitForDisplayed();
        await capture(`${theme}-${width}-invalid-search`);
        await click('button[aria-label="Clear all search filters"]');
        await galleryReady();

        await click('button[aria-label="Enable bulk actions"]');
        await click(`button[data-asset-id="${imageId}"]`);
        await $('#bulk-tag-draft-input').waitForEnabled();
        const inspectorWidth = await $('[data-testid="bulk-action-panel"]').getSize('width');
        assert.equal(inspectorWidth, 360);
        await capture(`${theme}-${width}-bulk`);
        await click('button[aria-label="Disable bulk actions"]');
        await click('button[aria-label="Open tag list"]');
        await $('#tag-list-filter-input').waitForDisplayed();
        await capture(`${theme}-${width}-tags`);
        await browser.keys('Escape');

        await click(`button[data-asset-id="${imageId}"]`);
        await $('#lightbox-tag-draft-input').waitForEnabled();
        await click('button[aria-label="Show info"]');
        await capture(`${theme}-${width}-lightbox`);
        await click('#lightbox-delete-button');
        await $('#lightbox-delete-confirm-input').waitForDisplayed();
        await capture(`${theme}-${width}-delete-confirmation`);
        await browser.keys('Escape');
        await $('#lightbox-delete-confirm-input').waitForExist({ reverse: true });
        await closeLightbox();

        await click('#open-settings-button');
        await click('.settings-nav a[href="#settings-duplicates"]');
        await click('button=Start');
        await $('#duplicate-resolver-heading').waitForDisplayed({ timeout: 20000 });
        await capture(`${theme}-${width}-duplicates`);
        await browser.keys('Escape');
        await click('.settings-nav a[href="#settings-danger"]');
        await click('button=Remove all thumbnails, indexed assets and scan paths');
        await $('#clear-library-confirm-heading').waitForDisplayed();
        await capture(`${theme}-${width}-clear-confirmation`);
        await browser.keys('Escape');
        await click('.settings-nav a[href="#settings-scan"]');
        await click('button=Remove');
        await $('#remove-scan-root-confirm-heading').waitForDisplayed();
        await capture(`${theme}-${width}-root-confirmation`);
        await browser.keys('Escape');
        await click('button[aria-label="Back"]');
      });
    }
  }
  for (const theme of ["dark", "light"]) {
    it(`keeps ${theme} lightbox drawers usable at 600px and 320px`, async () => {
      await browser.setWindowSize(1440, 900);
      await browser.refresh();
      await click('#open-settings-button');
      await $('#settings-theme-select').selectByAttribute('value', theme);
      await click('button[aria-label="Back"]');
      for (const [width, height] of [[600, 400], [320, 360]]) {
        await browser.setWindowSize(1440, 900);
        await galleryReady();
        await click(`button[data-asset-id="${imageId}"]`);
        await $('#lightbox-tag-draft-input').waitForEnabled();
        await browser.setWindowSize(width, height);
        await click('button[aria-label="Open asset panel"]');
        await browser.waitUntil(() => browser.execute(() => {
          const panel = document.querySelector('[data-lightbox-toolbar]');
          const rect = panel.getBoundingClientRect();
          const rail = document.querySelector('[data-testid="lightbox-action-rail"]').getBoundingClientRect();
          return rect.left >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight
            && panel.scrollWidth <= panel.clientWidth + 1 && rail.bottom <= rect.bottom;
        }));
        await capture(`${theme}-${width}-drawer`);
        await click('#lightbox-delete-button');
        await $('#lightbox-delete-confirm-input').waitForDisplayed();
        await capture(`${theme}-${width}-drawer-confirmation`);
        await browser.keys('Escape');
        await $('#lightbox-delete-confirm-input').waitForExist({ reverse: true });
        await browser.keys('Escape');
        await browser.waitUntil(async () => (await $('[data-lightbox-toolbar]').getAttribute('aria-hidden')) === 'true');
        await browser.keys('Escape');
        await $('[data-lightbox-kind]').waitForExist({ reverse: true });
      }
      await browser.setWindowSize(1440, 900);
    });
  }
  it('fits Polish settings and stacks the bulk inspector below 1000px', async () => {
    await browser.setWindowSize(1000, 720);
    await browser.refresh();
    await click('#open-settings-button');
    await $('#settings-language-select').selectByAttribute('value', 'pl');
    await browser.waitUntil(async () => (await $('html').getAttribute('lang')) === 'pl');
    await capture('pl-1000-settings');
    await click('button[aria-label="Wstecz"]');
    await galleryReady();
    await capture('pl-1000-gallery');
    await click(`button[data-asset-id="${imageId}"]`);
    await $('#lightbox-tag-draft-input').waitForEnabled();
    await capture('pl-1000-lightbox');
    await browser.keys('Escape');
    await click('#open-settings-button');
    await $('#settings-language-select').selectByAttribute('value', 'en');
    await click('button[aria-label="Back"]');
    await browser.setWindowSize(900, 720);
    await click('button[aria-label="Enable bulk actions"]');
    await $('[data-testid="bulk-action-panel"]').waitForExist();
    const layout = await browser.execute(() => {
      const gallery = document.querySelector('[data-testid="gallery-grid"]').getBoundingClientRect();
      const inspector = document.querySelector('[data-testid="bulk-action-panel"]').getBoundingClientRect();
      return { galleryBottom: gallery.bottom, inspectorTop: inspector.top, inspectorWidth: inspector.width };
    });
    assert.ok(layout.inspectorTop >= layout.galleryBottom - 1);
    assert.ok(layout.inspectorWidth > 800);
    await capture('light-900-stacked-bulk');
    await click('button[aria-label="Disable bulk actions"]');
    await browser.setWindowSize(1440, 900);
  });
  it('captures native controls and reports an unreadable original', async () => {
    await browser.setWindowSize(1440, 900);
    await browser.refresh();
    await galleryReady();
    await $('#gallery-media-kind').selectByAttribute('value', 'video');
    await $('button[data-asset-index="0"]').waitForDisplayed();
    await click('button[data-asset-index="0"]');
    await $('[data-lightbox-video-player]').waitForDisplayed();
    await capture('native-video-controls');
    await browser.keys('Escape');
    await $('[data-lightbox-kind]').waitForExist({ reverse: true });

    // Index a valid temporary original, then corrupt it as an external file change.
    await fs.copyFile(path.join(root, 'study-1.png'), path.join(root, 'unreadable-original.png'));
    await invoke('rescan_all_roots');
    const page = await invoke('list_assets', { offset: 0, limit: 200, tagsAnd: [], tagsNot: [], kind: 'image', favoritesOnly: false });
    const broken = page.items.find((asset) => asset.path === path.join(root, 'unreadable-original.png'));
    assert.ok(broken, 'The temporary original must be indexed before opening it');
    await fs.writeFile(path.join(root, 'unreadable-original.png'), 'invalid PNG fixture');
    await invoke('set_asset_tags', { assetId: broken.id, tags: ['unreadable-fixture'] });
    await browser.refresh();
    await $('.filter-input').setValue('unreadable-fixture');
    await click('button=Search');
    await click(`button[data-asset-id="${broken.id}"]`);
    await $('[data-testid="lightbox-media-error"]').waitForDisplayed();
    await capture('original-media-error');
    await closeLightbox();
  });

});
