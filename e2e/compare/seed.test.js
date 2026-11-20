import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { SEEDS, seedMedia, validateSeed } from "./seed.js";

async function freshRoot(t) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-compare-seed-test-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  return root;
}

test("knows the empty and the control seed", () => {
  assert.deepEqual(SEEDS, ["empty", "control"]);
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
});

test("refuses a directory that already holds files and an unknown seed", async (t) => {
  const root = await freshRoot(t);
  await fs.writeFile(path.join(root, "keep.png"), "x");
  for (const seed of SEEDS) await assert.rejects(seedMedia(root, seed), /not empty/);
  await fs.rm(path.join(root, "keep.png"));
  await assert.rejects(seedMedia(root, "personal"), /Unknown seed/);
  assert.deepEqual(await fs.readdir(root), []);
});
