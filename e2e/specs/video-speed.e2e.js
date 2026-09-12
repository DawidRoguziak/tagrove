import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createPlayableFixtures } from "../fixtures.js";

const run = promisify(execFile);
const suite = process.env.MEDIATAGGER_NATIVE_VIDEO === "1" ? describe : describe.skip;
const output = path.resolve("artifacts/video-speed", new Date().toISOString().replace(/[:.]/g, "-"));
const player = "[data-native-video-active]";
let root;
let ids;
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
async function rect(selector) {
  return browser.execute(selector => {
    const r = document.querySelector(selector).getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }, selector);
}
async function point(x, y) {
  await native("click", Math.round(x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0)), Math.round(y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0)));
  await browser.pause(120);
}
async function click(selector) {
  const r = await rect(selector);
  await point(r.x + r.width / 2, r.y + r.height / 2);
}
async function controls() {
  const r = await rect(player);
  return { r, speed: { x: r.x + r.width - 76, y: r.y + r.height - 29 }, play: { x: r.x + 32, y: r.y + r.height - 29 } };
}
async function menu() {
  const { speed } = await controls();
  await point(speed.x, speed.y);
  await browser.pause(350);
}
async function attribute(name) {
  return browser.execute(name => document.querySelector("[data-native-video-active]")?.getAttribute(`data-native-${name}`), name);
}
async function waitAttribute(name, value) {
  await browser.waitUntil(async () => await attribute(name) === String(value), { timeout: 5000, timeoutMsg: `native ${name}=${value}` });
}
async function capture(name) {
  const file = path.join(output, `${name}.png`);
  await run("import", ["-window", "root", file]);
  return file;
}
async function open(id) {
  const openedAt = Date.now();
  await $(`button[data-asset-id="${id}"]`).click();
  await $(player).waitForExist({ timeout: 15000 });
  let started;
  await browser.waitUntil(async () => {
    started = await browser.execute(() => {
      const p = document.querySelector("[data-native-video-active]");
      return p && { time: Number(p.dataset.nativeTime), rate: Number(p.dataset.nativeRate), paused: p.dataset.nativePaused };
    });
    return started?.time > 0 && started.paused === "false";
  }, { timeout: 10000 });
  const elapsed = (Date.now() - openedAt) / 1000;
  assert.ok(started.time <= elapsed * started.rate + .3,
    `reopen time ${started.time} exceeds elapsed playback ${elapsed * started.rate}`);
  await browser.pause(600);
}
async function close() {
  await click('button[aria-label="Close preview"]');
  await $(player).waitForExist({ reverse: true });
}

suite("native video speed and reopening", function () {
  this.timeout(600000);
  before(async () => {
    assert.equal(process.env.DISPLAY, process.env.MEDIATAGGER_NATIVE_DISPLAY, "native input requires the owned private display");
    assert.ok(process.env.XDG_DATA_HOME?.startsWith(os.tmpdir() + path.sep));
    await fs.mkdir(output, { recursive: true });
    console.log("VIDEO_SPEED_EVIDENCE", output);
    root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-video-speed-"));
    await createPlayableFixtures(root, "speed");
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: root });
    await invoke("rescan_all_roots");
    const page = await invoke("list_assets", { offset: 0, limit: 20, tagsAnd: [], tagsNot: [], kind: "video", favoritesOnly: false });
    ids = page.items.map(asset => asset.id);
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 15000 });
    await open(ids[0]);
    await capture("opened");
  });
  afterEach(async function () {
    if (this.currentTest.state === "failed") await capture("failure");
  });
  after(async () => {
    await invoke("clear_library_data");
    if (root) await fs.rm(root, { recursive: true, force: true });
  });
  it("dismisses outside clicks without activating the picture or controls", async () => {
    await menu();
    await capture("menu-open");
    const { r, play } = await controls();
    await point(r.x + r.width / 2, r.y + r.height / 2);
    await waitAttribute("paused", false);
    await capture("picture-dismissed");
    // A second picture click must work, proving capture released the sequence.
    await point(r.x + r.width / 2, r.y + r.height / 2);
    await waitAttribute("paused", true);
    await point(play.x, play.y);
    await waitAttribute("paused", false);
    await menu();
    await point(play.x, play.y);
    await waitAttribute("paused", false);
    await capture("controls-dismissed");
    await menu();
    const dx = Number(process.env.MEDIATAGGER_NATIVE_X ?? 0);
    const dy = Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0);
    await native("drag", Math.round(r.x + r.width / 2 + dx), Math.round(r.y + r.height / 2 + dy),
      Math.round(play.x + dx), Math.round(play.y + dy));
    await waitAttribute("paused", false);
    // Sidebar Close and backdrop each need a second click to close the lightbox.
    await menu();
    await click('button[aria-label="Close preview"]');
    assert.ok(await $(player).isExisting(), "dismissal must leave the video open");
    await close();
    await open(ids[0]);
    await menu();
    await capture("backdrop-menu-open");
    await point(5, 5);
    assert.ok(await $(player).isExisting(), "dismissal must leave the video open");
    await point(5, 5);
    await $(player).waitForExist({ reverse: true });
    await open(ids[0]);
  });
  it("selects every rate, toggles the trigger and consumes Escape before fullscreen", async () => {
    await menu();
    await menu();
    const { play } = await controls();
    await point(play.x, play.y);
    await waitAttribute("paused", true);
    await point(play.x, play.y);
    await menu();
    await native("key", "Escape");
    assert.ok(await $(player).isExisting(), "dismissal must leave the video open");
    await capture("escape-dismissed");
    await native("key", "f");
    await browser.waitUntil(async () => await attribute("fullscreen") !== null);
    await menu();
    await native("key", "Escape");
    assert.notEqual(await attribute("fullscreen"), null);
    await native("key", "Escape");
    await browser.waitUntil(async () => await attribute("fullscreen") === null);
    await capture("fullscreen-exited");
    await menu();
    await native("key", "Down");
    await native("key", "Return");
    await waitAttribute("rate", .75);
  });
  it("dismisses on focus loss and zero bounds and rejects stale bounds", async () => {
    await menu();
    await native("focus-away");
    const { play, r } = await controls();
    await point(play.x, play.y);
    await waitAttribute("paused", true);
    await point(play.x, play.y);
    await menu();
    const sessionId = Number(await attribute("session"));
    await invoke("set_video_bounds", { sessionId, bounds: { ...r, width: 0 } });
    await capture("zero-bounds");
    await invoke("set_video_bounds", { sessionId, bounds: r });
    await point(play.x, play.y);
    await waitAttribute("paused", true);
    await point(play.x, play.y);
    await assert.rejects(invoke("set_video_bounds", { sessionId: sessionId - 1, bounds: { ...r, width: 0 } }));
    await waitAttribute("paused", false);
  });
  it("keeps moving pixels and responsive controls through 30 speed and reopen cycles", async () => {
    const evidence = [];
    for (let cycle = 0; cycle < 30; cycle++) {
      const session = Number(await attribute("session"));
      const rate = [0.5, 0.75, 1, 1.25, 1.5, 2][cycle % 6];
      await menu();
      // Rows are located from the GTK popover screenshot, independently of IPC.
      const { speed } = await controls();
      await point(speed.x, speed.y - 270 + (cycle % 6) * 42);
      await waitAttribute("rate", rate);
      await waitAttribute("paused", false);
      const a = Number(await attribute("time"));
      await browser.waitUntil(async () => Number(await attribute("time")) !== a);
      const first = await capture(`cycle-${cycle}-a`);
      await browser.pause(250);
      const second = await capture(`cycle-${cycle}-b`);
      const r = await rect(player);
      const { stdout } = await run("python3", ["e2e/video-frames.py", first, second,
        r.x + Number(process.env.MEDIATAGGER_NATIVE_X ?? 0),
        r.y + Number(process.env.MEDIATAGGER_NATIVE_Y ?? 0), r.width, r.height].map(String));
      evidence.push({ cycle, session, rate, first, second, ...JSON.parse(stdout) });
      // Closing by IPC while open tests lifecycle cleanup, separately from outside clicks.
      if (cycle % 3 === 0) {
        await menu();
        await invoke("close_video", { sessionId: session });
      }
      await close();
      await open(ids[Math.floor(cycle / 2) % 2]);
      assert.notEqual(Number(await attribute("session")), session);
      await waitAttribute("rate", rate);
      await invoke("close_video", { sessionId: session });
      assert.ok(await $(player).isExisting(), "stale close leaves replacement visible");
    }
    await fs.writeFile(path.join(output, "cycles.json"), JSON.stringify(evidence, null, 2));
    await close();
  });
});
