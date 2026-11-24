import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  addControlDuplicate,
  CONTROL_DUPLICATE_DIRECTORY,
  CONTROL_DUPLICATE_NAME,
  CONTROL_DUPLICATE_SECONDS,
  CONTROL_SEED_BASE_SECONDS,
  CONTROL_SEED_ORDER,
  controlSeedModifiedSeconds,
  pinControlSeedOrder,
  SEEDS,
  seedMedia,
  validateSeed
} from "./seed.js";

const repositoryRoot = path.resolve(import.meta.dirname, "../..");

async function freshRoot(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-compare-seed-test-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  return root;
}

test("knows the empty, the control and the duplicates seed", () => {
  assert.deepEqual(SEEDS, ["empty", "control", "duplicates"]);
  for (const seed of SEEDS) assert.equal(validateSeed(seed), seed);
  for (const seed of ["", "personal", undefined, "Empty"])
    assert.throws(() => validateSeed(seed), /Unknown seed/, String(seed));
});

test("the empty seed leaves the directory empty", async (t) => {
  const root = await freshRoot(t);
  assert.deepEqual(await seedMedia(root, "empty"), []);
  assert.deepEqual(await fs.readdir(root), []);
});

// Needs ffmpeg, as the fixture tests of app:control do.
test("the control seed is the media app:control indexes for Tauri", async (t) => {
  const root = await freshRoot(t);
  const files = await seedMedia(root, "control");
  assert.deepEqual(files, [
    "control-1.png",
    "control-2.png",
    "control-3.png",
    "control-4.png",
    "control-playable-1.mp4",
    "control-playable-2.mp4",
    "control-playable.gif"
  ]);
  for (const file of files) assert.ok((await fs.stat(path.join(root, file))).size > 0, file);
  assert.deepEqual(await galleryOrder(root), CONTROL_SEED_ORDER);
});

// The order the gallery shows: modification time in whole seconds, newest first. Ties would fall
// to the asset ID, which the scan hands out in no fixed order, so the test fails on any tie.
async function galleryOrder(root) {
  const entries = [];
  for (const name of await fs.readdir(root)) {
    const { mtimeMs } = await fs.stat(path.join(root, name));
    entries.push({ name, seconds: Math.floor(mtimeMs / 1000) });
  }
  const seconds = entries.map((entry) => entry.seconds);
  assert.equal(new Set(seconds).size, seconds.length, `tied seconds: ${seconds}`);
  return entries.sort((a, b) => b.seconds - a.seconds).map((entry) => entry.name);
}

// Needs ffmpeg, as the control seed does.
test("the duplicates seed is the control seed and one more file of the same name", async (t) => {
  const root = await freshRoot(t);
  const entries = await seedMedia(root, "duplicates");
  assert.deepEqual(entries, [...CONTROL_SEED_ORDER, CONTROL_DUPLICATE_DIRECTORY].sort());
  assert.deepEqual(await fs.readdir(path.join(root, CONTROL_DUPLICATE_DIRECTORY)), [
    CONTROL_DUPLICATE_NAME
  ]);
  assert.ok(CONTROL_SEED_ORDER.includes(CONTROL_DUPLICATE_NAME));
  const copy = await fs.readFile(path.join(root, CONTROL_DUPLICATE_DIRECTORY, CONTROL_DUPLICATE_NAME));
  assert.deepEqual(copy, await fs.readFile(path.join(root, CONTROL_DUPLICATE_NAME)));
  // Older than every control file: the seven control tiles keep their places.
  const { mtimeMs } = await fs.stat(
    path.join(root, CONTROL_DUPLICATE_DIRECTORY, CONTROL_DUPLICATE_NAME)
  );
  assert.equal(mtimeMs, CONTROL_DUPLICATE_SECONDS * 1000);
  assert.ok(CONTROL_DUPLICATE_SECONDS < Math.min(...CONTROL_SEED_ORDER.map(controlSeedModifiedSeconds)));
});

test("a duplicate is added only to a directory that holds exactly the control seed", async (t) => {
  const root = await freshRoot(t);
  await assert.rejects(addControlDuplicate(root), /exactly the control seed/);
  for (const name of CONTROL_SEED_ORDER) await fs.writeFile(path.join(root, name), name);
  await fs.writeFile(path.join(root, "keep.png"), "x");
  await assert.rejects(addControlDuplicate(root), /exactly the control seed/);
  assert.deepEqual((await fs.readdir(root)).sort(), [...CONTROL_SEED_ORDER, "keep.png"].sort());
  await fs.rm(path.join(root, "keep.png"));
  assert.equal(
    await addControlDuplicate(root),
    path.join(CONTROL_DUPLICATE_DIRECTORY, CONTROL_DUPLICATE_NAME)
  );
  // A second call finds more than the control seed and writes nothing.
  await assert.rejects(addControlDuplicate(root), /exactly the control seed/);
});

test("the control seed order is a fixed rule over the file names", () => {
  assert.deepEqual(CONTROL_SEED_ORDER, [
    "control-playable.gif",
    "control-1.png",
    "control-2.png",
    "control-3.png",
    "control-4.png",
    "control-playable-1.mp4",
    "control-playable-2.mp4"
  ]);
  const seconds = CONTROL_SEED_ORDER.map(controlSeedModifiedSeconds);
  assert.equal(seconds[0], CONTROL_SEED_BASE_SECONDS);
  for (const [index, value] of seconds.entries()) {
    assert.ok(Number.isInteger(value), String(value));
    if (index) assert.ok(value < seconds[index - 1], "strictly older along the gallery order");
  }
  // A fixed instant in the past, not the time of the run.
  assert.ok(CONTROL_SEED_BASE_SECONDS * 1000 < Date.now() - 365 * 24 * 3600 * 1000);
  for (const name of ["control-5.png", "keep.png", "", "../control-1.png"])
    assert.throws(() => controlSeedModifiedSeconds(name), /Not a control seed file/, name);
});

test("pinning gives the same order whatever order and time the files were written in", async (t) => {
  const forward = await freshRoot(t);
  const backward = await freshRoot(t);
  for (const name of CONTROL_SEED_ORDER) await fs.writeFile(path.join(forward, name), name);
  for (const name of [...CONTROL_SEED_ORDER].reverse()) {
    await fs.writeFile(path.join(backward, name), name);
    // A newer write would otherwise come first: fake a spread of write times.
    const now = Date.now() / 1000 + CONTROL_SEED_ORDER.indexOf(name);
    await fs.utimes(path.join(backward, name), now, now);
  }
  await pinControlSeedOrder(forward);
  await pinControlSeedOrder(backward);
  assert.deepEqual(await galleryOrder(forward), CONTROL_SEED_ORDER);
  assert.deepEqual(await galleryOrder(backward), CONTROL_SEED_ORDER);
  for (const name of CONTROL_SEED_ORDER) {
    const [a, b] = await Promise.all(
      [forward, backward].map((root) => fs.stat(path.join(root, name)))
    );
    assert.equal(a.mtimeMs, controlSeedModifiedSeconds(name) * 1000, name);
    assert.equal(b.mtimeMs, a.mtimeMs, name);
  }
});

test("pinning refuses a directory that does not hold exactly the control seed", async (t) => {
  const root = await freshRoot(t);
  await assert.rejects(pinControlSeedOrder(root), /exactly the control seed/);
  for (const name of CONTROL_SEED_ORDER.slice(1)) await fs.writeFile(path.join(root, name), name);
  await assert.rejects(pinControlSeedOrder(root), /exactly the control seed/);
  await fs.writeFile(path.join(root, CONTROL_SEED_ORDER[0]), "x");
  await fs.writeFile(path.join(root, "keep.png"), "x");
  const before = (await fs.stat(path.join(root, "keep.png"))).mtimeMs;
  await assert.rejects(pinControlSeedOrder(root), /exactly the control seed/);
  for (const name of [...CONTROL_SEED_ORDER, "keep.png"]) {
    const { mtimeMs } = await fs.stat(path.join(root, name));
    assert.notEqual(Math.floor(mtimeMs / 1000), controlSeedModifiedSeconds(CONTROL_SEED_ORDER[0]));
  }
  assert.equal((await fs.stat(path.join(root, "keep.png"))).mtimeMs, before);
});

// app:control seeds the Tauri side itself; it must pin with the same helper, after writing the
// files and before the scan root is added.
test("app:control pins the control seed with the shared rule before it scans", async () => {
  const source = await fs.readFile(path.join(repositoryRoot, "e2e/control.js"), "utf8");
  assert.match(source, /^import \{ pinControlSeedOrder \} from "\.\/compare\/seed\.js";$/m);
  const written = source.indexOf(
    'await createPlayableFixtures(manifest.media, "control-playable");'
  );
  const pinned = source.indexOf("await pinControlSeedOrder(manifest.media);");
  const scanned = source.indexOf('await invoke("add_scan_root", { path: manifest.media });');
  assert.ok(written > 0 && pinned > written && scanned > pinned, { written, pinned, scanned });
  assert.equal(source.split("pinControlSeedOrder(").length - 1, 1, "one call");
});

test("refuses a directory that already holds files and an unknown seed", async (t) => {
  const root = await freshRoot(t);
  await fs.writeFile(path.join(root, "keep.png"), "x");
  for (const seed of SEEDS) await assert.rejects(seedMedia(root, seed), /not empty/);
  await fs.rm(path.join(root, "keep.png"));
  await assert.rejects(seedMedia(root, "personal"), /Unknown seed/);
  assert.deepEqual(await fs.readdir(root), []);
});
