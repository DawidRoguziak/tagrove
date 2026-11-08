import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";
import {
  COMMAND_ARITIES,
  COMMAND_CLASS,
  commandClass,
  dragActions,
  gateCommand,
  isScriptTimeout,
  liveGroupMembers,
  nativeEnvironment,
  parseDragTarget,
  parseWait,
  removeOwnedScratch,
  requireFreePort,
  SOCKET_PATH_LIMIT,
  stopOwnedGroup,
  validateRequest,
  validateSessionId,
  validateSocketPath,
  withModifiers
} from "./control-support.js";
import { copyPngFixtures, createPlayableFixtures } from "./fixtures.js";

const id = "a".repeat(32);
const execFileAsync = promisify(execFile);

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
      const videoPath = path.join(root, `playable-${index}.mp4`);
      const mp4 = await fs.readFile(videoPath);
      assert.equal(mp4.subarray(4, 8).toString(), "ftyp");
      const { stdout } = await execFileAsync(process.env.FFPROBE_PATH ?? "ffprobe", [
        "-v", "error", "-show_entries", "stream=codec_type", "-of", "json", videoPath
      ]);
      const { streams } = JSON.parse(stdout);
      assert.ok(streams.some((stream) => stream.codec_type === "video"), videoPath);
      assert.equal(streams.filter((stream) => stream.codec_type === "audio").length, 0, videoPath);
    }
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});

const nativeHelper = path.resolve(import.meta.dirname, "control-native.py");

test("new commands validate arity, ranges, and enumerations", () => {
  const ok = [
    ["modclick", ["Shift", "button"]],
    ["modclick", ["Control+Shift", "button"]],
    ["dblclick", ["li"]],
    ["drag", ["a", "b"]],
    ["drag", ["a", "-40,120", "150"]],
    ["drag", ["a", 'button[data-asset-index="3"]@4,-2', "3000"]],
    ["drag", ["a", "40,260", "ctrl"]],
    ["drag", ["a", "40,260", "200", "ctrl"]],
    ["wheel", [".gallery-scroll", "-5000"]],
    ["wheel", ["#tile-size", "8", "ctrl"]],
    ["select", ["#settings-language-select", "value=pl"]],
    ["select", ["#settings-language-select", "Polski"]],
    ["alert", ["accept"]],
    ["alert", ["text"]],
    ["alert", ["send", "YES"]],
    ["wait", ["idle"]],
    ["wait", ["idle", "90000"]],
    ["wait", ["#x", "hidden"]],
    ["wait", ["#x", "visible", "250"]],
    ["wait", ["#x", "5000"]],
    ["resize", ["320", "240"]],
    ["resize", ["1440", "900"]],
    ["native", ["key", "Control+Shift+a"]],
    ["native", ["type", "Hello, world!"]],
    ["native", ["click", "0", "899", "3"]],
    ["native", ["move", "1439", "0"]],
    ["native", ["drag", "1", "2", "3", "4"]],
    ["native", ["wheel", "10", "10", "-3", "ctrl"]]
  ];
  for (const [command, args] of ok) assert.doesNotThrow(() => validateRequest({ command, args }), `${command} ${args}`);
  const bad = [
    ["modclick", ["Hyper", "button"]],
    ["modclick", ["Shift+Control", "button"]],
    ["modclick", ["Shift"]],
    ["drag", ["a", "b", "NaN"]],
    ["drag", ["a", "b", "149"]],
    ["drag", ["a", "b", "3001"]],
    ["drag", ["a", "99999,1"]],
    ["drag", ["", "b"]],
    ["drag", ["a", "b", "ctrl", "200"]],
    ["drag", ["a", "b", "200", "300"]],
    ["drag", ["a", "b", "shift"]],
    ["wheel", ["a", "0"]],
    ["wheel", ["a", "5001"]],
    ["wheel", ["a", "1.5"]],
    ["wheel", ["a", "10", "alt"]],
    ["select", ["a", "value="]],
    ["alert", ["close"]],
    ["alert", ["send"]],
    ["alert", ["accept", "extra"]],
    ["wait", ["idle", "visible"]],
    ["wait", ["#x", "present"]],
    ["wait", ["#x", "visible", "90001"]],
    ["wait", ["#x", "visible", "249"]],
    ["wait", ["#x", "NaN"]],
    ["resize", ["319", "240"]],
    ["resize", ["1441", "900"]],
    ["resize", ["800", "901"]],
    ["native", ["key", "Control+Shift+Alt+Super+a"]],
    ["native", ["key", "Return;rm"]],
    ["native", ["type", "zażółć"]],
    ["native", ["type", "tab\there"]],
    ["native", ["click", "1440", "0"]],
    ["native", ["click", "-1", "0"]],
    ["native", ["click", "1", "1", "4"]],
    ["native", ["wheel", "1", "1", "0"]],
    ["native", ["wheel", "1", "1", "51"]],
    ["native", ["exec", "x"]],
    ["native", ["drag", "1", "2", "3"]]
  ];
  for (const [command, args] of bad) assert.throws(() => validateRequest({ command, args }), undefined, `${command} ${args}`);
});

test("every command has exactly one class and alert text only observes", () => {
  assert.deepEqual(Object.keys(COMMAND_CLASS).sort(), Object.keys(COMMAND_ARITIES).sort());
  for (const kind of Object.values(COMMAND_CLASS)) assert.ok(["observe", "act", "control"].includes(kind));
  assert.equal(commandClass({ command: "alert", args: ["text"] }), "observe");
  assert.equal(commandClass({ command: "alert", args: ["accept"] }), "act");
  for (const command of ["inspect", "screenshot", "logs", "wait", "doctor"]) assert.equal(COMMAND_CLASS[command], "observe");
  for (const command of ["stop", "refresh", "diagnose"]) assert.equal(COMMAND_CLASS[command], "control");
});

test("session states gate commands", () => {
  const request = (command, ...args) => ({ command, args });
  for (const command of ["click", "native", "refresh", "inspect", "wait", "doctor"])
    assert.ok(gateCommand("tainted", request(command, "x")), command);
  for (const command of ["logs", "screenshot", "stop"]) assert.equal(gateCommand("tainted", request(command)), null);
  assert.equal(gateCommand("alert-open", request("alert", "accept")), null);
  assert.equal(gateCommand("alert-open", request("screenshot")), null);
  assert.ok(gateCommand("alert-open", request("click", "x")));
  assert.ok(gateCommand("alert-open", request("inspect")));
  assert.equal(gateCommand("busy-maintenance", request("inspect")), null);
  assert.equal(gateCommand("busy-maintenance", request("wait", "idle")), null);
  assert.match(gateCommand("busy-maintenance", request("click", "x")), /^busy-maintenance/);
  assert.ok(gateCommand("busy-maintenance", request("refresh")));
  assert.equal(gateCommand("ok", request("native", "key", "a")), null);
  assert.ok(isScriptTimeout(new Error("script timeout: Timed out waiting for asynchronous script result")));
  assert.equal(isScriptTimeout(new Error("Refusing unexpected app identity/profile")), false);
});

test("drag holds at least 150 ms and starts with a move of at least 5 px", () => {
  for (const [start, end, hold] of [
    [{ x: 10, y: 10 }, { x: 300, y: 600 }, undefined],
    [{ x: 10.4, y: 10.6 }, { x: 10, y: 11 }, 150],
    [{ x: 50, y: 50 }, { x: 50, y: 50 }, 3000],
    [{ x: 500, y: 500 }, { x: 20, y: 480 }, 400]
  ]) {
    const actions = dragActions(start, end, hold);
    assert.deepEqual(actions.map((action) => action.type), ["pointerMove", "pointerDown", "pause", "pointerMove", "pointerMove", "pointerUp"]);
    assert.ok(actions[2].duration >= 150);
    const moved = Math.hypot(actions[3].x - actions[0].x, actions[3].y - actions[0].y);
    assert.ok(moved >= 5, `first move ${moved}`);
    assert.deepEqual([actions[4].x, actions[4].y], [Math.round(end.x), Math.round(end.y)]);
    for (const action of actions) if ("x" in action) assert.ok(Number.isInteger(action.x) && Number.isInteger(action.y));
  }
  assert.throws(() => dragActions({ x: 0, y: 0 }, { x: 9, y: 9 }, 100));
  assert.deepEqual(parseDragTarget("sel@-3,4"), { selector: "sel", dx: -3, dy: 4 });
  assert.deepEqual(parseDragTarget("12,-7"), { selector: null, dx: 12, dy: -7 });
  assert.deepEqual(parseDragTarget('a[title="x@y"]'), { selector: 'a[title="x@y"]', dx: 0, dy: 0 });
  assert.deepEqual(parseWait(["#a"]), { target: "#a", state: "displayed", timeout: 15000 });
});

test("modifiers are released and actions released even when the action or keyDown fails", async () => {
  for (const failure of ["action", "keyDown", "none"]) {
    const calls = [];
    const browser = {
      async performActions(sources) {
        const kinds = sources[0].actions.map((action) => `${action.type}:${action.value.codePointAt(0).toString(16)}`);
        calls.push(kinds.join(","));
        if (failure === "keyDown" && kinds[0].startsWith("keyDown")) throw new Error("driver down");
      },
      async releaseActions() {
        calls.push("release");
      }
    };
    const action = async () => {
      calls.push("action");
      if (failure === "action") throw new Error("click intercepted");
      return "clicked";
    };
    const run = withModifiers(browser, ["Control", "Shift"], action);
    if (failure === "none") assert.equal(await run, "clicked");
    else await assert.rejects(run);
    assert.equal(calls.at(-1), "release", failure);
    assert.ok(calls.includes("keyUp:e008,keyUp:e009"), `${failure}: ${calls}`);
  }
});

test("the native environment comes only from the manifest", () => {
  const saved = { DISPLAY: process.env.DISPLAY, XAUTHORITY: process.env.XAUTHORITY };
  process.env.DISPLAY = ":0";
  process.env.XAUTHORITY = "/elsewhere/Xauthority";
  try {
    const env = nativeEnvironment({ id, display: ":42", scratch: "/tmp/media-tagger-control-abc", extra: "ignored" });
    assert.deepEqual(env, {
      PATH: "/usr/bin:/bin",
      DISPLAY: ":42",
      MEDIATAGGER_NATIVE_DISPLAY: ":42",
      XAUTHORITY: "/tmp/media-tagger-control-abc/Xauthority",
      XDG_DATA_HOME: "/tmp/media-tagger-control-abc/data",
      MEDIATAGGER_CONTROL_SCRATCH: "/tmp/media-tagger-control-abc",
      MEDIATAGGER_CONTROL_ID: id
    });
    for (const display of [":0", ":1", "remote:5", undefined])
      assert.throws(() => nativeEnvironment({ id, display, scratch: "/tmp/media-tagger-control-abc" }));
    assert.throws(() => nativeEnvironment({ id, display: ":42", scratch: "relative" }));
    assert.throws(() => nativeEnvironment({ id: "../x", display: ":42", scratch: "/tmp/x" }));
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
});

test("the native helper refuses before opening any display", async () => {
  const scratch = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-")));
  const runNative = async (env, args) => {
    try {
      const { stdout } = await execFileAsync("python3", [nativeHelper, ...args], { env });
      return { code: 0, stdout, stderr: "" };
    } catch (error) {
      return { code: error.code, stdout: error.stdout, stderr: error.stderr };
    }
  };
  try {
    await fs.writeFile(path.join(scratch, "owner"), id);
    // :59999 has no server; a refusal must happen before any attempt to open it.
    const env = nativeEnvironment({ id, display: ":59999", scratch });
    assert.deepEqual(await runNative(env, ["--check", "key", "Return"]), { code: 0, stdout: "control-native: ok\n", stderr: "" });
    const refusals = [
      [{ ...env, DISPLAY: ":0", MEDIATAGGER_NATIVE_DISPLAY: ":0" }, ["key", "Return"]],
      [{ ...env, MEDIATAGGER_NATIVE_DISPLAY: ":58888" }, ["key", "Return"]],
      [{ ...env, XAUTHORITY: path.join(os.tmpdir(), "Xauthority") }, ["key", "Return"]],
      [{ ...env, XDG_DATA_HOME: path.join(scratch, "other") }, ["key", "Return"]],
      [{ ...env, MEDIATAGGER_CONTROL_ID: "b".repeat(32) }, ["key", "Return"]],
      [{ ...env, MEDIATAGGER_CONTROL_SCRATCH: path.join(os.tmpdir(), "elsewhere") }, ["key", "Return"]],
      [env, ["type", "zażółć"]],
      [env, ["click", "1440", "1"]],
      [env, ["key", "Return;id"]],
      [env, ["wheel", "1", "1", "0"]]
    ];
    for (const [candidate, args] of refusals) {
      const result = await runNative(candidate, args);
      assert.equal(result.code, 2, `${JSON.stringify(args)} ${result.stderr}`);
      assert.match(result.stderr, /^control-native: refusing: /);
      assert.doesNotMatch(result.stderr, /cannot open/);
    }
  } finally {
    await fs.rm(scratch, { recursive: true, force: true });
  }
});

test("cleanup works with a TMPDIR outside /tmp and refuses the old parent", async () => {
  const parent = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-tmpdir-")));
  const custom = path.join(parent, "q");
  await fs.mkdir(custom);
  const saved = process.env.TMPDIR;
  try {
    const outside = await fs.mkdtemp(path.join(os.tmpdir(), "media-tagger-control-"));
    await fs.writeFile(path.join(outside, "owner"), id);
    process.env.TMPDIR = custom;
    const root = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), "media-tagger-control-"));
    assert.equal(path.dirname(root), custom);
    await fs.writeFile(path.join(root, "owner"), id);
    await assert.rejects(removeOwnedScratch(outside, id), /Refusing/);
    await removeOwnedScratch(root, id);
    await assert.rejects(fs.stat(root), { code: "ENOENT" });
    if (saved === undefined) delete process.env.TMPDIR;
    else process.env.TMPDIR = saved;
    await removeOwnedScratch(outside, id);
  } finally {
    if (saved === undefined) delete process.env.TMPDIR;
    else process.env.TMPDIR = saved;
    await fs.rm(parent, { recursive: true, force: true });
  }
});

test("socket paths longer than the Linux limit are refused with a readable error", () => {
  const ok = `/${"a".repeat(SOCKET_PATH_LIMIT - 1)}`;
  assert.equal(validateSocketPath(ok), ok);
  assert.throws(() => validateSocketPath(`${ok}b`), /108 bytes; Linux allows 107\. Start with a shorter TMPDIR/);
  // Multibyte characters count as bytes, not code units.
  assert.throws(() => validateSocketPath(`/${"ż".repeat(54)}`), /bytes/);
});
