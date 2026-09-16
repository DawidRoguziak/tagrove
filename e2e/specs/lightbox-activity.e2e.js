import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createPlayableFixtures } from "../fixtures.js";

const run = promisify(execFile);
const suite = process.env.MEDIATAGGER_LIGHTBOX_ACTIVITY === "1" ? describe : describe.skip;
const evidence = path.resolve("artifacts/lightbox-activity", new Date().toISOString().replace(/[:.]/g, "-"));
let root;
let assets;

async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw Error(result.error);
  return result.value;
}
async function native(action, ...args) {
  await run("python3", ["e2e/native-input.py", action, ...args.map(String)]);
}
async function click(selector) {
  await $(selector).waitForExist({ timeout: 10000 });
  let r;
  await browser.waitUntil(async () => {
    r = await browser.execute(selector => {
      const r = document.querySelector(selector).getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    }, selector);
    return r.width > 0 && r.height > 0;
  }, { timeout: 10000, timeoutMsg: `visible layout for ${selector}` });
  const window = await browser.getWindowRect();
  const x = Math.round(window.x + r.x + r.width / 2);
  const y = Math.round(window.y + r.y + r.height / 2);
  await native("move", x, y);
  await native("click", x, y);
}
async function sample() {
  return browser.execute(() => {
    const rect = el => {
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    };
    const control = selector => {
      const el = document.querySelector(selector);
      if (!el) return null;
      const css = getComputedStyle(el);
      const button = el.querySelector('button:not(:disabled)');
      const r = button.getBoundingClientRect();
      return { opacity: Number(css.opacity), pointerEvents: css.pointerEvents,
        transition: css.transitionDuration, clickable: button.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)) };
    };
    const headerClose = document.querySelector('#lightbox-sidebar[aria-hidden="false"] header button[aria-label="Close preview"]');
    return {
      floating: control('.lightbox-media-controls'), row: control('.lightbox-action-row'),
      stage: rect(document.querySelector('[data-lightbox-media-stage]')),
      media: rect(document.querySelector('[data-native-video-active], [data-testid="lightbox-image"]')),
      session: document.querySelector('[data-native-video-active]')?.dataset.nativeSession ?? null,
      panelClose: Boolean(headerClose && getComputedStyle(headerClose).opacity === "1"),
      focus: document.activeElement?.id,
      reduced: matchMedia('(prefers-reduced-motion: reduce)').matches
    };
  });
}
async function opacity(value) {
  await browser.waitUntil(async () => (await sample()).row.opacity === value, { timeout: 5500, timeoutMsg: `action row opacity ${value}` });
}

suite("lightbox activity desktop", function () {
  this.timeout(240000);
  before(async () => {
    assert.equal(process.env.DISPLAY, process.env.MEDIATAGGER_NATIVE_DISPLAY);
    assert.ok(process.env.XDG_DATA_HOME?.startsWith('/tmp/mediatagger-'));
    await fs.mkdir(evidence, { recursive: true });
    console.log("LIGHTBOX_ACTIVITY_EVIDENCE", evidence);
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-lightbox-activity-media-"));
    await createPlayableFixtures(root, "motion");
    await run(process.env.FFMPEG_PATH ?? "ffmpeg", ["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=640x480", "-frames:v", "1", path.join(root, "image.png")]);
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    assets = (await invoke("list_assets", { offset: 0, limit: 20, tagsAnd: [], tagsNot: [], favoritesOnly: false })).items;
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-id]').waitForDisplayed({ timeout: 20000 });
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") {
      await run('import', ['-window', 'root', path.join(evidence, 'failure.png')]);
    }
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });

  for (const width of [600, 1000]) {
    it(`keeps the bottom box hovered and places group copy in the sidebar, ${width}px`, async () => {
      await browser.refresh();
      await browser.setWindowSize(width, 720);
      const id = assets.find(asset => asset.kind === "image").id;
      await click(`button[data-asset-id="${id}"]`);
      await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-testid="lightbox-image"]')?.naturalWidth > 0));
      if (width < 768) await click('#lightbox-sidebar-trigger-button');
      const input = await $('#lightbox-media-group-key-input');
      await input.setValue('  synthetic-hover-group  ');
      const placement = await browser.execute(() => {
        const input = document.querySelector('#lightbox-media-group-key-input');
        const copy = input.nextElementSibling;
        const uuid = copy.nextElementSibling;
        const rect = el => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
        return { input: rect(input), copy: rect(copy), uuid: rect(uuid), label: copy.getAttribute('aria-label'),
          railCopy: Boolean(document.querySelector('.lightbox-action-island button[aria-label="Copy group name"]')) };
      });
      assert.equal(placement.label, 'Copy group name');
      assert.equal(placement.railCopy, false);
      assert.equal(placement.copy.width, 32);
      assert.equal(placement.copy.height, 32);
      assert.ok(placement.input.x < placement.copy.x && placement.copy.x < placement.uuid.x);
      assert.equal(placement.copy.y, placement.uuid.y);
      await click('#lightbox-media-group-key-input + button');
      await $('button[aria-label="Group name copied"]').waitForExist();
      await run('import', ['-window', 'root', path.join(evidence, `copy-${width}.png`)]);
      await click('#lightbox-sidebar-close-button');
      await browser.pause(300);
      const move = async (selector, gap = false) => {
        const r = await browser.execute(selector => {
          const r = document.querySelector(selector).getBoundingClientRect();
          return { x: r.x, y: r.y, width: r.width, height: r.height };
        }, selector);
        const window = await browser.getWindowRect();
        await native('move', Math.round(window.x + r.x + (gap ? 2 : r.width / 2)), Math.round(window.y + r.y + r.height / 2));
      };
      // Metadata and the island's padding both hold the whole box open.
      for (const [selector, gap] of [['.lightbox-media-summary', false], ['.lightbox-action-island', true]]) {
        await move(selector, gap);
        await browser.pause(3500);
        const held = await sample();
        assert.equal(held.row.opacity, 1);
        assert.equal(held.row.clickable, true);
        assert.equal(held.floating.opacity, 0);
      }
      await run('import', ['-window', 'root', path.join(evidence, `hover-${width}.png`)]);
      // Empty surrounding row space exits the box and does not hold it open.
      // Capture before the deadline in-page: software WebKit driver calls can
      // take longer than the full inactivity interval.
      await browser.execute(() => {
        const island = document.querySelector('.lightbox-action-island');
        island.addEventListener('pointerleave', () => {
          setTimeout(() => {
            island.dataset.exitOpacity = getComputedStyle(island.parentElement).opacity;
          }, 500);
        }, { once: true });
      });
      await move('.lightbox-action-row', true);
      await opacity(0);
      assert.equal(await $('.lightbox-action-island').getAttribute('data-exit-opacity'), '1');
      await fs.writeFile(path.join(evidence, `copy-hover-${width}.json`), JSON.stringify({ placement, hidden: await sample() }, null, 2));
      await click('.lightbox-media-controls button[aria-label="Close preview"]');
      await $('[data-lightbox-kind]').waitForExist({ reverse: true });
    });
  }

  for (const theme of ["light", "dark"]) {
    for (const width of [600, 1000]) {
      for (const kind of ["video", "image", "gif"]) {
        it(`fades controls and restores focus without resizing ${kind}, ${theme}, ${width}px`, async () => {
          await browser.refresh();
          await browser.setWindowSize(width, 720);
          await $('#open-settings-button').waitForDisplayed();
          await $('#open-settings-button').click();
          await $(`.theme-choice:has(input[value="${theme}"])`).click();
          await $('button[aria-label="Back"]').click();
          const id = assets.find(asset => asset.kind === kind).id;
          await $(`button[data-asset-id="${id}"]`).waitForDisplayed();
          await click(`button[data-asset-id="${id}"]`);
          if (kind === "video") await $('[data-native-video-active]').waitForExist({ timeout: 15000 });
          else await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-testid="lightbox-image"]')?.naturalWidth > 0));
          if (width < 768) await click('#lightbox-sidebar-trigger-button');
          await $('#lightbox-sidebar[aria-hidden="false"]').waitForExist();
          await opacity(0);
          const panel = await sample();
          assert.equal(panel.panelClose, true);
          assert.equal(panel.floating, null);
          assert.equal(panel.reduced, process.env.MEDIATAGGER_REDUCED_MOTION === "1");
          await run('import', ['-window', 'root', path.join(evidence, `${theme}-${width}-${kind}-panel.png`)]);
          // Opening focuses collapse. Keyboard collapse must restore the reopen button.
          await click('#lightbox-sidebar-close-button');
          // The wide media column expands after the drawer's 200 ms exit.
          await browser.pause(300);
          await click('#lightbox-sidebar-trigger-button');
          await browser.waitUntil(async () => (await sample()).focus === 'lightbox-sidebar-close-button');
          await native('key', 'Return');
          await browser.waitUntil(async () => (await sample()).focus === 'lightbox-sidebar-trigger-button');
          await browser.pause(300);
          // Start a fresh idle interval after drawer geometry has settled. A driver
          // round trip can take several seconds on software-rendered WebKit.
          await native('key', 'Shift_L');
          let shown;
          await browser.waitUntil(async () => {
            shown = await sample();
            return shown.floating?.opacity === 1 && shown.row.opacity === 1;
          });
          assert.equal(shown.floating.opacity, 1);
          assert.equal(shown.floating.clickable, true);
          assert.equal(shown.row.clickable, true);
          assert.equal(shown.row.transition, shown.reduced ? '0s' : '0.2s');
          assert.equal(shown.floating.transition, shown.reduced ? '0s' : '0.2s');
          await opacity(0);
          const hidden = await sample();
          assert.equal(hidden.floating.opacity, 0);
          assert.equal(hidden.floating.pointerEvents, 'none');
          assert.equal(hidden.row.pointerEvents, 'none');
          assert.equal(hidden.floating.clickable, false);
          assert.equal(hidden.row.clickable, false);
          assert.equal(hidden.focus, 'lightbox-sidebar-trigger-button');
          assert.deepEqual(hidden.stage, shown.stage);
          assert.deepEqual(hidden.media, shown.media);
          assert.equal(hidden.session, shown.session);
          // The very first key after idle both reveals and activates the focused trigger.
          await native('key', 'Return');
          await $('#lightbox-sidebar[aria-hidden="false"]').waitForExist();
          await browser.waitUntil(async () => (await sample()).focus === 'lightbox-sidebar-close-button');
          await native('key', 'Return');
          await browser.waitUntil(async () => (await sample()).focus === 'lightbox-sidebar-trigger-button');
          await browser.pause(300);
          await run('import', ['-window', 'root', path.join(evidence, `${theme}-${width}-${kind}.png`)]);
          await fs.writeFile(path.join(evidence, `${theme}-${width}-${kind}.json`), JSON.stringify({ panel, shown, hidden }, null, 2));
          await click('.lightbox-media-controls button[aria-label="Close preview"]');
          await $('[data-lightbox-kind]').waitForExist({ reverse: true });
        });
      }
    }
  }
});
