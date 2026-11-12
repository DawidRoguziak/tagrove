import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createPlayableFixtures } from "../fixtures.js";
import { listAssetsForAssertions } from "../assets.js";
import { assertPrivateDisplay, captureDisplay } from "../display.js";

const run = promisify(execFile);
const output = path.resolve("artifacts/native-video-escape", new Date().toISOString().replace(/[:.]/g, "-"));
const suite = process.env.MEDIATAGGER_NATIVE_VIDEO === "1" ? describe : describe.skip;
const player = "[data-native-video-active]";
const observations = [];
let root;
let assetId;
async function invoke(command, payload = {}) {
  const result = await browser.executeAsync((command, payload, done) => {
    window.__TAURI_INTERNALS__.invoke(command, payload).then(
      value => done({ value }), error => done({ error: String(error) }));
  }, command, payload);
  if (result.error) throw Error(result.error);
  return result.value;
}
async function native(action, ...args) {
  await run("python3", ["e2e/native-input.py", action, ...args.map(String)]);
}
async function point(x, y) {
  await native("click", Math.round(x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0)),
    Math.round(y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0)));
  await browser.pause(160);
}
async function clickDom(selector) {
  const r = await browser.execute(selector => {
    const r = document.querySelector(selector).getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  }, selector);
  // Moving to the current pointer position does not reset the inactivity timer.
  // Approach from a distinct pixel even when the pointer already rests on the target.
  await native("move", Math.round(r.x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0)) - 1,
    Math.round(r.y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0)));
  await native("move", Math.round(r.x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0)),
    Math.round(r.y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0)));
  await browser.waitUntil(() => browser.execute(selector => {
    const element = document.querySelector(selector);
    const r = element.getBoundingClientRect();
    return r.right <= innerWidth && element.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
  }, selector), { timeoutMsg: `Native pointer can reach ${selector}` });
  const target = await browser.execute(selector => {
    const r = document.querySelector(selector).getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  }, selector);
  await point(target.x, target.y);
}
async function rect() {
  return browser.execute(() => {
    const r = document.querySelector("[data-native-video-active]").getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  });
}
async function state() {
  return browser.execute(() => {
    const p = document.querySelector("[data-native-video-active]");
    return p ? { session: p.dataset.nativeSession, paused: p.dataset.nativePaused,
      volume: Number(p.dataset.nativeVolume), fullscreen: p.hasAttribute("data-native-fullscreen") } : null;
  });
}
async function waitState(property, value) {
  await browser.waitUntil(async () => (await state())?.[property] === value,
    { timeout: 5000, timeoutMsg: `Native snapshot ${property}=${value}` });
}
async function counters() {
  return browser.execute(() => ({ ...window.__verifyEscapeCounters,
    hasFocus: document.hasFocus(), activeTag: document.activeElement?.tagName,
    activePlayer: document.activeElement?.hasAttribute("data-lightbox-video-player") }));
}
async function capture(label) {
  await captureDisplay(path.join(output, `${label}.png`));
}
async function record(label, extra = {}) {
  const value = { label, state: await state(), dom: await counters(), ...extra };
  observations.push(value);
  console.log("NATIVE_ESCAPE", JSON.stringify(value));
  await fs.writeFile(path.join(output, "observations.json"), `${JSON.stringify(observations, null, 2)}\n`);
  return value;
}
async function open() {
  await $(`button[data-asset-id="${assetId}"]`).click();
  await $(player).waitForExist({ timeout: 15000 });
  await waitState("paused", "false");
  await browser.pause(600);
  await browser.execute(() => {
    window.__verifyEscapeCounters = { clicks: 0, keys: [] };
    window.__verifyEscapeKey = event => window.__verifyEscapeCounters.keys.push(event.key);
    window.__verifyEscapeClick = () => window.__verifyEscapeCounters.clicks++;
    window.addEventListener("keydown", window.__verifyEscapeKey, true);
    window.addEventListener("click", window.__verifyEscapeClick, true);
  });
}
async function focusPlay() {
  const r = await rect();
  await point(r.x + 32, r.y + r.height - 29);
  await waitState("paused", "true");
  await native("key", "space");
  await waitState("paused", "false");
  await native("key", "space");
  await waitState("paused", "true");
  const dom = await counters();
  assert.deepEqual(dom.keys, [], "Space must activate GTK play button without reaching DOM");
  assert.equal(dom.clicks, 0, "Physical GTK play click must bypass DOM");
  await record("play-focus-proven", { proof: "Physical play click pauses, two Space keys resume then pause, no DOM click/key" });
}
suite("native video Escape routing", function () {
  this.timeout(120000);
  before(async () => {
    assertPrivateDisplay();
    assert.ok(process.env.XDG_DATA_HOME?.startsWith("/tmp/mediatagger-"));
    assert.equal(await invoke("plugin:window|title", { label: "main" }), "Tagrove E2E");
    await fs.mkdir(output, { recursive: true });
    console.log("NATIVE_ESCAPE_EVIDENCE", output);
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-native-escape-"));
    await createPlayableFixtures(root, "native-escape");
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await listAssetsForAssertions({
      offset: 0, limit: 20, tagsAnd: [], tagsNot: [], kind: "video", favoritesOnly: false });
    assetId = page.items[0].id;
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 15000 });
    await browser.setWindowSize(1000, 720);
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") await capture("failure");
    await browser.execute(() => {
      window.removeEventListener("keydown", window.__verifyEscapeKey, true);
      window.removeEventListener("click", window.__verifyEscapeClick, true);
    });
    if ((await state())?.fullscreen) {
      await native("key", "Escape");
      await browser.pause(300);
      if ((await state())?.fullscreen) await native("key", "Escape");
      await waitState("fullscreen", false);
    }
    const close = $('button[aria-label="Close preview"]');
    if (await close.isExisting()) await clickDom('button[aria-label="Close preview"]');
    await $(player).waitForExist({ reverse: true, timeout: 10000 });
    await browser.pause(300);
  });
  after(async () => {
    await fs.writeFile(path.join(output, "observations.json"), `${JSON.stringify(observations, null, 2)}\n`);
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  it("closes with physical WebView picture focus and physical Escape", async () => {
    await open();
    const r = await rect();
    await point(r.x + r.width / 2, r.y + r.height / 2);
    await waitState("paused", "true");
    const dom = await counters();
    assert.equal(dom.clicks, 1);
    assert.equal(dom.activePlayer, true);
    await capture("webview-before");
    await record("webview-before");
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
    const result = await record("webview-after");
    assert.deepEqual(result.dom.keys, ["Escape"]);
    await capture("webview-after");
  });
  it("closes windowed preview after Escape with proven GTK play keyboard focus", async () => {
    await open();
    await focusPlay();
    assert.equal((await state()).fullscreen, false);
    await capture("play-before");
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
    assert.deepEqual((await counters()).keys, []);
    await record("play-after-escape");
    await capture("play-after");
  });
  it("closes windowed preview after Escape with proven GTK volume keyboard focus", async () => {
    await open();
    const r = await rect();
    await point(r.x + 150, r.y + r.height - 29);
    await native("key", "End");
    await waitState("volume", 1);
    await native("key", "Home");
    await waitState("volume", 0);
    const dom = await counters();
    assert.deepEqual(dom.keys, [], "GTK volume End/Home changes must bypass DOM");
    assert.equal(dom.clicks, 0);
    await record("volume-focus-proven", { proof: "Native End sets one, Home sets zero, no DOM keys/clicks" });
    await capture("volume-before");
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
    assert.deepEqual((await counters()).keys, []);
    await record("volume-after-escape");
    await capture("volume-after");
  });
  it("consumes windowed speed menu Escape while preserving the session", async () => {
    await open();
    await focusPlay();
    const before = await state();
    const r = await rect();
    await point(r.x + r.width - 76, r.y + r.height - 29);
    await browser.pause(350);
    await capture("windowed-menu-open");
    await native("key", "Escape");
    await browser.pause(350);
    const result = await record("windowed-menu-after-escape");
    assert.equal(result.state?.session, before.session);
    assert.equal(result.state.fullscreen, false);
    assert.deepEqual(result.dom.keys, []);
    await capture("windowed-menu-closed");
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
  });
  it("dismisses fullscreen menu first and exits fullscreen next without closing", async () => {
    await open();
    await focusPlay();
    const before = await state();
    await native("key", "f");
    await waitState("fullscreen", true);
    const r = await rect();
    await point(r.x + r.width - 76, r.y + r.height - 29);
    await browser.pause(350);
    await capture("fullscreen-menu-open");
    await native("key", "Escape");
    await browser.pause(350);
    const first = await record("fullscreen-first-escape");
    assert.equal(first.state?.session, before.session);
    assert.equal(first.state.fullscreen, true);
    await capture("fullscreen-menu-closed");
    await native("key", "Escape");
    await waitState("fullscreen", false);
    const second = await record("fullscreen-second-escape");
    assert.equal(second.state?.session, before.session);
    assert.deepEqual(second.dom.keys, []);
    await capture("fullscreen-exited");
  });
  it("dismisses delete confirmation before closing after GTK focus", async () => {
    await open();
    await clickDom('button[aria-label="Delete media"]');
    await $('[data-testid="lightbox-delete-confirm-dialog"]').waitForDisplayed();
    await browser.execute(() => { window.__verifyEscapeCounters = { clicks: 0, keys: [] }; });
    await focusPlay();
    await native("key", "Escape");
    await $('[data-testid="lightbox-delete-confirm-dialog"]').waitForExist({ reverse: true });
    assert.ok(await state(), "confirmation Escape retains playback");
    await capture("confirmation-dismissed");
    // Confirmation restores focus to its WebView delete trigger.
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
  });
  it("closes the narrow drawer before the preview with physical WebView Escape", async () => {
    await browser.setWindowSize(600, 720);
    await open();
    await clickDom('button[aria-label="Open asset panel"]');
    await $('#lightbox-sidebar[aria-hidden="false"]').waitForDisplayed();
    await clickDom("#lightbox-sidebar header");
    assert.equal(await browser.execute(() => document.hasFocus() && innerWidth <= 767), true);
    await native("key", "Escape");
    await browser.waitUntil(async () => await $('#lightbox-sidebar-close-button').getAttribute("aria-expanded") === "false");
    assert.ok(await state(), "drawer Escape retains playback");
    await capture("drawer-dismissed");
    await native("key", "Escape");
    await $(player).waitForExist({ reverse: true, timeout: 5000 });
    await browser.setWindowSize(1000, 720);
  });
  it("reveals idle controls when the native pointer already rests on the target", async () => {
    await browser.setWindowSize(600, 720);
    await open();
    const session = (await state()).session;
    for (const label of ["Open asset panel", "Close preview"]) {
      const selector = `button[aria-label="${label}"]`;
      const position = await browser.execute(selector => {
        const r = document.querySelector(selector).getBoundingClientRect();
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
      }, selector);
      await native("move", Math.round(position.x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0)),
        Math.round(position.y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0)));
      await browser.waitUntil(() => browser.execute(() => {
        const style = getComputedStyle(document.querySelector(".lightbox-media-controls"));
        return style.opacity === "0" && style.pointerEvents === "none";
      }), { timeout: 10000, timeoutMsg: "Floating controls must finish their inactivity fade" });
      await record(`idle-${label}`, { position, opacity: 0, pointerEvents: "none" });
      await clickDom(selector);
      if (label === "Open asset panel") {
        await $('#lightbox-sidebar[aria-hidden="false"]').waitForDisplayed();
        assert.equal((await state()).session, session);
        await clickDom("#lightbox-sidebar header");
        await native("key", "Escape");
        await browser.waitUntil(async () => await $('#lightbox-sidebar-close-button').getAttribute("aria-expanded") === "false");
      } else {
        await $(player).waitForExist({ reverse: true, timeout: 5000 });
      }
      await record(`revealed-${label}`);
    }
    await capture("idle-controls-recovered");
    await browser.setWindowSize(1000, 720);
  });

});
