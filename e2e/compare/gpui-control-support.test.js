import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  gpuiE2eBinaryPath,
  gpuiE2eIdentifier,
  gpuiE2eTargetDir,
  gpuiE2eWindowTitle,
  projectRoot
} from "../../scripts/gpui-build-e2e.mjs";
import { removeOwnedScratch as removeAppControlScratch } from "../control-support.js";
import {
  acquireLock,
  APP_CONTROL_LOCK,
  assertBakedIdentifier,
  assertWindowIdentity,
  createSettleTracker,
  desktopConflict,
  displayEnvironment,
  findDesktopE2eRunner,
  parseImageInfo,
  parseStartArguments,
  parseWindowProperties,
  profileDirectory,
  releaseLock,
  removeOwnedScratch,
  resetScenarioState,
  SCENARIO_MEDIA_VARIABLE,
  SCRATCH_PREFIX,
  sessionEnvironment,
  validateGpuiIdentity,
  validateRequest,
  validateScenarioId
} from "./gpui-control-support.js";

const id = "a".repeat(32);
const identity = {
  identifier: gpuiE2eIdentifier,
  windowTitle: gpuiE2eWindowTitle,
  targetDir: gpuiE2eTargetDir,
  binaryPath: gpuiE2eBinaryPath
};

async function scratchRoot(t, owner = id, prefix = SCRATCH_PREFIX) {
  const root = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), prefix));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(path.join(root, "owner"), owner);
  return root;
}

test("accepts only the documented commands and scenario ids", () => {
  for (const request of [
    { command: "doctor", args: [] },
    { command: "scenario", args: ["shell-empty"] },
    { command: "screenshot", args: [] },
    { command: "logs", args: [] },
    { command: "stop", args: [] }
  ])
    assert.equal(validateRequest(request), request);
  for (const request of [
    null,
    { command: "__proto__", args: [] },
    { command: "click", args: ["button"] },
    { command: "start", args: [] },
    { command: "scenario", args: [] },
    { command: "scenario", args: ["shell-empty", "extra"] },
    { command: "scenario", args: [42] },
    { command: "screenshot", args: ["file.png"] },
    { command: "stop" }
  ])
    assert.throws(() => validateRequest(request), /Invalid control command/, JSON.stringify(request));
  for (const invalid of ["", "../other", "Shell-Empty", "shell--empty", "-shell", "shell empty", "a".repeat(65), 7])
    assert.throws(() => validateScenarioId(invalid), /scenario id/, String(invalid));
});

test("start takes nothing or one scenario", () => {
  assert.deepEqual(parseStartArguments([]), { scenario: null });
  assert.deepEqual(parseStartArguments(["--scenario", "shell-empty"]), { scenario: "shell-empty" });
  for (const args of [["shell-empty"], ["--scenario"], ["--scenario", "../x"], ["--scenario", "a", "b"]])
    assert.throws(() => parseStartArguments(args), /Invalid control command/, args.join(" "));
});

test("the build script's E2E identity is the one the controller launches", () => {
  assert.doesNotThrow(() => validateGpuiIdentity(identity, projectRoot));
});

test("refuses another identifier, title, target directory or binary", () => {
  const root = "/synthetic/checkout";
  const good = {
    identifier: "com.example.mediatagger.gpui.e2e",
    windowTitle: "Tagrove GPUI E2E",
    targetDir: "/synthetic/checkout/src-gpui/target-e2e",
    binaryPath: "/synthetic/checkout/src-gpui/target-e2e/debug/media_tagger_gpui"
  };
  assert.doesNotThrow(() => validateGpuiIdentity(good, root));
  for (const identifier of [
    "com.example.mediatagger",
    "com.example.mediatagger.dev",
    "com.example.mediatagger.e2e",
    "com.example.mediatagger.gpui.dev",
    "com.example.mediatagger.gpui",
    "",
    undefined
  ])
    assert.throws(
      () => validateGpuiIdentity({ ...good, identifier }, root),
      /expected isolated identifier/,
      String(identifier)
    );
  for (const windowTitle of ["Tagrove", "Tagrove E2E", "Tagrove GPUI Dev", "", undefined])
    assert.throws(
      () => validateGpuiIdentity({ ...good, windowTitle }, root),
      /expected isolated window title/,
      String(windowTitle)
    );
  for (const targetDir of [
    "/synthetic/checkout/src-gpui/target",
    "/synthetic/checkout/src-tauri/target-e2e",
    "/synthetic/other/src-gpui/target-e2e",
    "/synthetic/checkout/src-gpui/target-e2e/debug",
    undefined
  ])
    assert.throws(
      () => validateGpuiIdentity({ ...good, targetDir }, root),
      /unsafe target directory/,
      String(targetDir)
    );
  for (const binaryPath of [
    "/synthetic/checkout/src-gpui/target/debug/media_tagger_gpui",
    "/synthetic/checkout/src-tauri/target-e2e/debug/media_tagger",
    "/synthetic/checkout/src-gpui/target-e2e/release/media_tagger_gpui",
    undefined
  ])
    assert.throws(
      () => validateGpuiIdentity({ ...good, binaryPath }, root),
      /binary outside the E2E target/,
      String(binaryPath)
    );
});

test("a binary baked for another profile is not launched", () => {
  assert.equal(assertBakedIdentifier("com.example.mediatagger.gpui.e2e\n"), gpuiE2eIdentifier);
  for (const output of ["com.example.mediatagger.gpui.dev\n", "com.example.mediatagger.e2e", "", undefined])
    assert.throws(() => assertBakedIdentifier(output), /Refusing to launch a binary/, String(output));
});

test("the application environment stays inside the session", () => {
  const scratch = `/tmp/${SCRATCH_PREFIX}abc`;
  const env = sessionEnvironment(
    {
      PATH: "/usr/bin",
      HOME: "/synthetic/home",
      WAYLAND_DISPLAY: "wayland-0",
      WAYLAND_SOCKET: "5",
      DISPLAY: ":0",
      XAUTHORITY: "/synthetic/home/.Xauthority",
      XDG_DATA_HOME: "/synthetic/home/.local/share",
      XDG_RUNTIME_DIR: "/run/user/1000",
      DBUS_SESSION_BUS_ADDRESS: "unix:path=/run/user/1000/bus"
    },
    { scratch, display: ":93" }
  );
  assert.deepEqual(env, {
    PATH: "/usr/bin",
    HOME: "/synthetic/home",
    DISPLAY: ":93",
    XAUTHORITY: `${scratch}/Xauthority`,
    XDG_DATA_HOME: `${scratch}/data`,
    XDG_CONFIG_HOME: `${scratch}/config`,
    XDG_CACHE_HOME: `${scratch}/cache`,
    XDG_RUNTIME_DIR: `${scratch}/runtime`,
    DBUS_SESSION_BUS_ADDRESS: `unix:path=${scratch}/runtime/bus`,
    LANGUAGE: "en",
    LC_ALL: "C.UTF-8",
    LANG: "C.UTF-8",
    [SCENARIO_MEDIA_VARIABLE]: `${scratch}/media`
  });
  for (const manifest of [
    { scratch, display: ":0" },
    { scratch, display: ":1" },
    { scratch, display: "remote:93" },
    { scratch },
    { scratch: "relative/root", display: ":93" },
    { scratch: "/tmp/media-tagger-control-abc", display: ":93" },
    undefined
  ])
    assert.throws(() => sessionEnvironment({}, manifest), /Refusing/, JSON.stringify(manifest));
});

test("X tools get their display and authority from the manifest only", () => {
  const scratch = `/tmp/${SCRATCH_PREFIX}abc`;
  assert.deepEqual(displayEnvironment({ scratch, display: ":93" }), {
    PATH: "/usr/bin:/bin",
    DISPLAY: ":93",
    MEDIATAGGER_NATIVE_DISPLAY: ":93",
    XAUTHORITY: `${scratch}/Xauthority`
  });
  assert.equal(displayEnvironment({ scratch, display: ":93" }, "/opt/bin").PATH, "/opt/bin");
  for (const display of [":0", ":1", "", undefined, ":93.0"])
    assert.throws(() => displayEnvironment({ scratch, display }), /private display/, String(display));
});

test("removes only its own marked scratch root", async (t) => {
  const owned = await scratchRoot(t);
  const foreignOwner = await scratchRoot(t, "b".repeat(32));
  const appControl = await scratchRoot(t, id, "media-tagger-control-");
  const unmarked = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), SCRATCH_PREFIX));
  t.after(() => fs.rm(unmarked, { recursive: true, force: true }));
  const nested = path.join(owned, `${SCRATCH_PREFIX}nested`);
  await fs.mkdir(nested);
  await fs.writeFile(path.join(nested, "owner"), id);
  const link = path.join(await fs.realpath(os.tmpdir()), `${SCRATCH_PREFIX}link-${process.pid}`);
  await fs.symlink(owned, link);
  t.after(() => fs.rm(link, { force: true }));

  for (const root of [foreignOwner, appControl, unmarked, nested, link])
    await assert.rejects(removeOwnedScratch(root, id), /Refusing unsafe|ENOENT/, root);
  await assert.rejects(removeOwnedScratch(owned, "../other"), /session ID/);
  // The two controllers cannot remove each other's roots.
  await assert.rejects(removeAppControlScratch(owned, id), /Refusing unsafe/);
  for (const root of [foreignOwner, appControl, unmarked, nested])
    assert.ok((await fs.stat(root)).isDirectory(), root);

  await removeOwnedScratch(owned, id);
  await assert.rejects(fs.stat(owned), { code: "ENOENT" });
});

test("a scenario reset touches the profile and the media directory only", async (t) => {
  const root = await scratchRoot(t);
  const profile = path.join(root, "data", gpuiE2eIdentifier);
  const neighbour = path.join(root, "data", "com.example.mediatagger.e2e");
  for (const dir of [profile, neighbour, path.join(root, "media"), path.join(root, "config")])
    await fs.mkdir(dir, { recursive: true });
  for (const file of [
    path.join(profile, "media.db"),
    path.join(neighbour, "media.db"),
    path.join(root, "media", "old.png"),
    path.join(root, "config", "keep")
  ])
    await fs.writeFile(file, "x");

  assert.deepEqual(await resetScenarioState(root, id, gpuiE2eIdentifier), {
    profile,
    media: path.join(root, "media")
  });

  await assert.rejects(fs.stat(profile), { code: "ENOENT" });
  assert.deepEqual(await fs.readdir(path.join(root, "media")), []);
  assert.equal(await fs.readFile(path.join(neighbour, "media.db"), "utf8"), "x");
  assert.equal(await fs.readFile(path.join(root, "config", "keep"), "utf8"), "x");
  assert.equal(await fs.readFile(path.join(root, "owner"), "utf8"), id);

  for (const identifier of ["com.example.mediatagger", "com.example.mediatagger.e2e", "..", "", undefined]) {
    assert.throws(() => profileDirectory(root, identifier), /Refusing unsafe GPUI app data cleanup/);
    await assert.rejects(resetScenarioState(root, id, identifier), /Refusing unsafe GPUI app data cleanup/);
  }
  await assert.rejects(resetScenarioState(root, "b".repeat(32), gpuiE2eIdentifier), /Refusing unsafe/);
  assert.equal(await fs.readFile(path.join(neighbour, "media.db"), "utf8"), "x");
});

test("reads the window identity from xprop output", () => {
  const output = [
    'WM_NAME(STRING) = "Tagrove GPUI E2E"',
    '_NET_WM_NAME(UTF8_STRING) = "Tagrove GPUI E2E"',
    'WM_CLASS(STRING) = "com.example.mediatagger.gpui.e2e", "com.example.mediatagger.gpui.e2e"',
    "_NET_WM_PID(CARDINAL) = 4242",
    ""
  ].join("\n");
  const properties = parseWindowProperties(output);
  assert.deepEqual(properties, {
    title: "Tagrove GPUI E2E",
    wmClass: ["com.example.mediatagger.gpui.e2e", "com.example.mediatagger.gpui.e2e"],
    pid: 4242
  });
  assert.equal(assertWindowIdentity(properties, { pid: 4242 }), properties);

  // The class is set after the title; an incomplete identity is not accepted.
  const early = parseWindowProperties(
    'WM_NAME(STRING) = "Tagrove GPUI E2E"\nWM_CLASS:  not found.\n_NET_WM_PID(CARDINAL) = 4242\n'
  );
  assert.deepEqual(early, { title: "Tagrove GPUI E2E", wmClass: [], pid: 4242 });
  assert.throws(() => assertWindowIdentity(early, { pid: 4242 }), /unexpected window class/);
  assert.deepEqual(parseWindowProperties(""), { title: null, wmClass: [], pid: null });

  assert.throws(() => assertWindowIdentity(properties, { pid: 4243 }), /window of process 4242/);
  assert.throws(
    () => assertWindowIdentity({ ...properties, title: "Tagrove E2E" }, { pid: 4242 }),
    /unexpected window title/
  );
  assert.throws(
    () => assertWindowIdentity({ ...properties, title: "Tagrove GPUI Dev" }, { pid: 4242 }),
    /unexpected window title/
  );
  assert.throws(
    () =>
      assertWindowIdentity(
        { ...properties, wmClass: ["com.example.mediatagger.gpui.dev", "com.example.mediatagger.gpui.dev"] },
        { pid: 4242 }
      ),
    /unexpected window class/
  );
});

async function fakeProc(t, processes) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-gpui-control-test-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  for (const [pid, argv, cwd] of processes) {
    await fs.mkdir(path.join(root, pid));
    await fs.writeFile(path.join(root, pid, "cmdline"), `${argv.join("\0")}\0`);
    if (cwd) await fs.symlink(cwd, path.join(root, pid, "cwd"));
  }
  await fs.mkdir(path.join(root, "self"));
  return root;
}

test("finds the desktop E2E launcher of this checkout only", async (t) => {
  const checkout = "/synthetic/checkout";
  const launcher = ["node", "/synthetic/checkout/node_modules/.bin/wdio", "run", "./e2e/wdio.conf.js"];
  const find = async (processes) =>
    findDesktopE2eRunner(checkout, { procRoot: await fakeProc(t, processes) });

  assert.equal(await find([["41", launcher, checkout]]), 41);
  assert.equal(
    await find([["42", ["node", "wdio", "run", "/synthetic/checkout/e2e/wdio.conf.js"], "/"]]),
    42
  );
  assert.equal(await find([["43", ["node", "wdio", "run", "e2e/wdio.conf.js"], checkout]]), 43);
  assert.equal(
    await find([["49", ["node", "node_modules/@wdio/cli/bin/wdio.js", "run", "e2e/wdio.conf.js"], checkout]]),
    49
  );
  // Another checkout, another config, an editor on the config, a process without a cwd.
  assert.equal(await find([["44", launcher, "/synthetic/other"]]), null);
  assert.equal(await find([["45", ["node", "wdio", "run", "./e2e/other.conf.js"], checkout]]), null);
  assert.equal(await find([["46", ["vim", "./e2e/wdio.conf.js"], checkout]]), null);
  assert.equal(await find([["47", launcher]]), null);
  assert.equal(await find([]), null);
  assert.equal(findDesktopE2eRunner(checkout, { procRoot: "/synthetic/missing-proc" }), null);
  // A process of another user is not this checkout's run.
  assert.equal(
    findDesktopE2eRunner(checkout, {
      procRoot: await fakeProc(t, [["48", launcher, checkout]]),
      uid: (process.getuid?.() ?? 0) + 1
    }),
    null
  );
});

test("a session does not start next to app:control or desktop E2E", async (t) => {
  const checkout = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-gpui-conflict-test-"));
  t.after(() => fs.rm(checkout, { recursive: true, force: true }));
  const idle = await fakeProc(t, [["50", ["node", "server.js"], checkout]]);
  const running = await fakeProc(t, [
    ["51", ["node", "wdio", "run", "./e2e/wdio.conf.js"], checkout]
  ]);

  assert.equal(desktopConflict(checkout, { procRoot: idle }), null);
  assert.match(desktopConflict(checkout, { procRoot: running }), /test:e2e:tauri is running.*process 51/);

  const lock = path.join(checkout, APP_CONTROL_LOCK);
  await fs.mkdir(path.dirname(lock), { recursive: true });
  await fs.writeFile(lock, JSON.stringify({ id, pid: 1 }));
  assert.match(desktopConflict(checkout, { procRoot: idle }), /app-control\/active\.lock exists/);
  // The check only reads: the foreign lock is still there.
  assert.equal(await fs.readFile(lock, "utf8"), JSON.stringify({ id, pid: 1 }));
});

test("the checkout lock is exclusive and only its owner removes it", async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-gpui-lock-test-"));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const lockPath = path.join(directory, "active.lock");

  const first = await acquireLock(lockPath, { id, pid: 100 });
  assert.deepEqual(JSON.parse(await fs.readFile(lockPath, "utf8")), { id, pid: 100 });
  await assert.rejects(acquireLock(lockPath, { id: "b".repeat(32), pid: 101 }), /may already own this checkout/);
  assert.deepEqual(JSON.parse(await fs.readFile(lockPath, "utf8")), { id, pid: 100 });
  assert.equal(await releaseLock(lockPath, first, id), true);
  await assert.rejects(fs.stat(lockPath), { code: "ENOENT" });

  // A lock that names another session is left alone.
  const second = await acquireLock(lockPath, { id, pid: 100 });
  await fs.writeFile(lockPath, JSON.stringify({ id: "b".repeat(32), pid: 101 }));
  assert.equal(await releaseLock(lockPath, second, id), false);
  assert.deepEqual(JSON.parse(await fs.readFile(lockPath, "utf8")), { id: "b".repeat(32), pid: 101 });

  // A lock that is already gone is not an error.
  await fs.rm(lockPath);
  const third = await acquireLock(lockPath, { id, pid: 100 });
  await fs.rm(lockPath);
  assert.equal(await releaseLock(lockPath, third, id), false);
});

test("the display has settled after two equal captures that differ from the empty display", () => {
  const settled = createSettleTracker("empty");
  assert.deepEqual(
    ["empty", "empty", "frame-1", "frame-2", "frame-2", "frame-3", "frame-3"].map(settled),
    [false, false, false, false, true, false, true]
  );
});

test("reads signature and size of a capture", () => {
  const signature = "0123456789abcdef".repeat(4);
  assert.deepEqual(parseImageInfo(`${signature} 1440 900\n`), { signature, width: 1440, height: 900 });
  for (const text of ["", "1440 900", `${signature} 1440`, `${signature.toUpperCase()} 1440 900`, undefined])
    assert.throws(() => parseImageInfo(text), /Unexpected image description/, String(text));
});
