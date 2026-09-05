import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import {
  liveGroupMembers,
  removeOwnedScratch,
  requireFreePort,
  stopOwnedGroup,
  validateRequest,
  validateSessionId
} from "./control-support.js";
import { copyPngFixtures, createPlayableFixtures } from "./fixtures.js";

const id = "a".repeat(32);

test("rejects paths as session IDs and malformed commands", () => {
  for (const invalid of ["../other", "", "/tmp/test", undefined]) {
    assert.throws(() => validateSessionId(invalid));
  }
  for (const invalid of [
    null,
    { command: "__proto__", args: [] },
    { command: "click", args: [] },
    { command: "fill", args: ["input", 42] },
    { command: "exec", args: ["anything"] }
  ]) {
    assert.throws(() => validateRequest(invalid));
  }
  assert.equal(validateRequest({ command: "fill", args: ["input", ""] }).args[1], "");
});

test("occupied local port is refused without disturbing its owner", async () => {
  const server = net.createServer((socket) => socket.end("unrelated"));
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  try {
    await assert.rejects(requireFreePort(port), /occupied/);
    assert.equal(server.listening, true);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
  await requireFreePort(port);
});

test("cleanup rejects a wrong owner and preserves external symlink targets", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-test-"));
  const other = await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-unrelated-"));
  try {
    await fs.writeFile(path.join(root, "owner"), id);
    await fs.writeFile(path.join(other, "sentinel"), "keep");
    await assert.rejects(removeOwnedScratch(root, "b".repeat(32)), /Refusing/);
    await fs.symlink(other, path.join(root, "external"));
    await removeOwnedScratch(root, id);
    assert.equal(await fs.readFile(path.join(other, "sentinel"), "utf8"), "keep");
  } finally {
    await fs.rm(root, { recursive: true, force: true });
    await fs.rm(other, { recursive: true, force: true });
  }
});

test("cleanup refuses a symlink as the scratch root", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-target-"));
  const link = `${root}-link`;
  try {
    await fs.writeFile(path.join(root, "owner"), id);
    await fs.symlink(root, link);
    await assert.rejects(removeOwnedScratch(link, id), /Refusing/);
    assert.equal(await fs.readFile(path.join(root, "owner"), "utf8"), id);
  } finally {
    await fs.rm(link, { force: true });
    await fs.rm(root, { recursive: true, force: true });
  }
});

test("stops an owned process tree, supports repeated cleanup, leaves unrelated process alive", async () => {
  const owned = spawn(
    process.execPath,
    [
      "-e",
      `
    require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio:'ignore'});
    setInterval(() => {}, 1000);
  `
    ],
    { detached: true, stdio: "ignore" }
  );
  const other = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
    detached: true,
    stdio: "ignore"
  });
  try {
    for (let attempt = 0; (await liveGroupMembers(owned.pid)).length < 2 && attempt < 50; attempt++)
      await delay(20);
    assert.ok((await liveGroupMembers(owned.pid)).length >= 2);
    await stopOwnedGroup(owned);
    await stopOwnedGroup(owned);
    assert.deepEqual(await liveGroupMembers(owned.pid), []);
    assert.ok((await liveGroupMembers(other.pid)).includes(other.pid));
  } finally {
    await stopOwnedGroup(owned);
    await stopOwnedGroup(other);
  }
});

test("shared fixtures create real media exclusively in the supplied temporary root", async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-fixture-"));
  try {
    await copyPngFixtures(root, "sample", 4);
    await createPlayableFixtures(root, "playable");
    assert.equal((await fs.readdir(root)).length, 7);
    const png = await fs.readFile(path.join(root, "sample-1.png"));
    assert.equal(png.subarray(1, 4).toString(), "PNG");
    const gif = await fs.readFile(path.join(root, "playable.gif"));
    assert.equal(gif.subarray(0, 3).toString(), "GIF");
    for (const index of [1, 2]) {
      const mp4 = await fs.readFile(path.join(root, `playable-${index}.mp4`));
      assert.equal(mp4.subarray(4, 8).toString(), "ftyp");
    }
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});
