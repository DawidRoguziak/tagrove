import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { removeOwnedScratch } from "./gpui-control-support.js";

// The controllers in a disposable copy of the checkout, with a fake `bun` that records its
// calls: every case here must be refused before anything is built, locked or started, so no
// case needs a display.

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const id = "a".repeat(32);
const appControlLock = "artifacts/app-control/active.lock";
const gpuiControlLock = "artifacts/gpui-control/active.lock";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mediatagger-compare-guard-test-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const copy = (file) => {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(repositoryRoot, file), target);
  };
  for (const file of [
    "package.json",
    "scripts/gpui-build-e2e.mjs",
    "src-gpui/crates/media_tagger_gpui/src/profile.rs",
    "docs/plans/gpui-port/epics.json",
    "e2e/build-e2e.js",
    "e2e/control.js",
    "e2e/control-support.js",
    "e2e/display.js",
    "e2e/fixtures.js"
  ])
    copy(file);
  const compare = path.join(repositoryRoot, "e2e/compare");
  for (const name of fs.readdirSync(compare))
    if (name.endsWith(".js") && !name.endsWith(".test.js")) copy(`e2e/compare/${name}`);
  for (const name of fs.readdirSync(path.join(compare, "scenarios")))
    copy(`e2e/compare/scenarios/${name}`);
  fs.mkdirSync(path.join(root, "bin"));
  fs.writeFileSync(path.join(root, "bin", "bun"), '#!/bin/sh\nprintf "%s\\n" "$*" >> calls\nexit 1\n', {
    mode: 0o755
  });
  const environment = {
    ...process.env,
    NODE_TEST_CONTEXT: undefined,
    FFMPEG_PATH: undefined,
    PATH: `${path.join(root, "bin")}:${process.env.PATH}`
  };
  const run = (script, args) =>
    spawnSync(process.execPath, [script, ...args], {
      cwd: root,
      encoding: "utf8",
      timeout: 60_000,
      env: environment
    });
  const writeLock = (file, content = JSON.stringify({ id, pid: 1 })) => {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), content);
    return content;
  };
  return {
    root,
    writeLock,
    control: (...args) => run("e2e/compare/gpui-control.js", args),
    spawnControl: (...args) =>
      spawn(process.execPath, ["e2e/compare/gpui-control.js", ...args], {
        cwd: root,
        stdio: ["ignore", "ignore", "pipe"],
        env: environment
      }),
    compare: (...args) => run("e2e/compare/compare.js", args),
    read: (file) => (fs.existsSync(path.join(root, file)) ? fs.readFileSync(path.join(root, file), "utf8") : null),
    exists: (file) => fs.existsSync(path.join(root, file)),
    edit: (file, from, to) => {
      const target = path.join(root, file);
      const source = fs.readFileSync(target, "utf8");
      assert.ok(source.includes(from), `${file} does not contain ${from}`);
      fs.writeFileSync(target, source.replace(from, to));
    }
  };
}

// Stand-ins for the build and the X tools, so that `start` gets as far as its Xvfb without a
// display: the build succeeds, the binary names the E2E identifier, and Xvfb records its
// process ID in `xvfb-started` and then never reports ready.
function fakeSessionTools(f) {
  const script = (file, body) => {
    fs.mkdirSync(path.dirname(path.join(f.root, file)), { recursive: true });
    fs.writeFileSync(path.join(f.root, file), `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  };
  for (const tool of ["bun", "xauth", "xprop", "import", "magick", "ffmpeg"]) script(`bin/${tool}`, "exit 0");
  script("bin/Xvfb", '[ "$1" = "-help" ] && exit 0\necho "$$" >> xvfb-started\nexec sleep 30');
  script("src-gpui/target-e2e/debug/media_tagger_gpui", "echo com.example.mediatagger.gpui.e2e");
}

async function until(condition, what, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (!condition()) {
    assert.ok(Date.now() < deadline, `timed out waiting for ${what}`);
    await delay(20);
  }
}

// Nothing was built, locked or left behind by a refused start.
function assertNothingStarted(f) {
  assert.equal(f.read("calls"), null, "a build tool ran");
  assert.equal(f.exists("artifacts/gpui-compare"), false);
  assert.deepEqual(
    fs.existsSync(path.join(f.root, "artifacts/gpui-control"))
      ? fs.readdirSync(path.join(f.root, "artifacts/gpui-control")).filter((name) => name !== "active.lock")
      : [],
    []
  );
}

test("a GPUI session does not start while app:control owns the checkout", (t) => {
  const f = fixture(t);
  const lock = f.writeLock(appControlLock);

  const result = f.control("start");

  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /Refusing to start a GPUI control session: artifacts\/app-control\/active\.lock exists/);
  assert.doesNotMatch(result.stdout, /BUILD|READY/);
  assert.equal(f.read(appControlLock), lock, "the foreign lock was touched");
  assert.equal(f.exists(gpuiControlLock), false);
  assertNothingStarted(f);
});

test("a GPUI session does not start while desktop E2E runs in the checkout", async (t) => {
  const f = fixture(t);
  // Stands in for the wdio launcher: a script named wdio, the same arguments and the same
  // working directory.
  fs.writeFileSync(path.join(f.root, "bin", "wdio"), "setTimeout(() => {}, 60000);\n");
  const launcher = spawn(process.execPath, ["bin/wdio", "run", "./e2e/wdio.conf.js"], {
    cwd: f.root,
    stdio: "ignore"
  });
  t.after(() => launcher.kill());
  await new Promise((resolve, reject) => {
    launcher.once("spawn", resolve);
    launcher.once("error", reject);
  });

  const result = f.control("start");

  assert.equal(result.status, 1, result.stderr);
  assert.match(
    result.stderr,
    new RegExp(`test:e2e:tauri is running in this checkout \\(process ${launcher.pid}\\)`)
  );
  assert.equal(launcher.exitCode, null, "the launcher was signalled");
  assert.equal(f.exists(gpuiControlLock), false);
  assertNothingStarted(f);

  const compared = f.compare("shell-empty");
  assert.equal(compared.status, 1, compared.stderr);
  assert.match(compared.stderr, /Refusing to compare: bun run test:e2e:tauri is running/);
  assertNothingStarted(f);
});

test("a second GPUI session is refused and the first one's lock stays", (t) => {
  const f = fixture(t);
  const lock = f.writeLock(gpuiControlLock, JSON.stringify({ id: "b".repeat(32), pid: 1 }));

  const result = f.control("start");

  assert.equal(result.status, 1, result.stderr);
  assert.equal(f.read(gpuiControlLock), lock);
  assertNothingStarted(f);
  // `start` probes the machine's desktop driver ports before it looks at its own lock. When
  // another checkout or tool holds one, that refusal comes first and this case cannot be shown.
  if (/Port \d+ is occupied/.test(result.stderr)) {
    t.skip("a desktop driver port is taken on this machine, so the lock was not reached");
    return;
  }
  assert.match(result.stderr, /A GPUI controller may already own this checkout/);
});

test("a start interrupted while its Xvfb comes up starts no second display", async (t) => {
  const f = fixture(t);
  fakeSessionTools(f);
  const controller = f.spawnControl("start");
  let stderr = "";
  controller.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const exited = () => controller.exitCode !== null || controller.signalCode !== null;
  const started = () => (f.read("xvfb-started") ?? "").split("\n").filter(Boolean).map(Number);
  const sessions = () =>
    f.exists("artifacts/gpui-control")
      ? fs.readdirSync(path.join(f.root, "artifacts/gpui-control")).filter((name) => name !== "active.lock")
      : [];
  const manifest = () => JSON.parse(f.read(`artifacts/gpui-control/${sessions()[0]}/session.json`));
  t.after(async () => {
    if (!exited()) controller.kill("SIGKILL");
    // Only what this copy of the checkout started and marked as its own.
    for (const pid of started()) {
      try {
        // The copy may be gone by now; the kernel then marks the link as deleted.
        const cwd = fs.readlinkSync(`/proc/${pid}/cwd`).replace(/ \(deleted\)$/, "");
        if (cwd === f.root) process.kill(pid, "SIGKILL");
      } catch {}
    }
    if (sessions().length) await removeOwnedScratch(manifest().scratch, sessions()[0]).catch(() => {});
  });

  await until(() => started().length > 0 || exited(), "the session's Xvfb");
  if (exited() && /Port \d+ is occupied/.test(stderr)) {
    t.skip("a desktop driver port is taken on this machine, so no session can start");
    return;
  }
  assert.equal(exited(), false, stderr);
  const [xvfb] = started();
  const { scratch } = manifest();
  assert.equal(fs.existsSync(scratch), true);

  controller.kill("SIGTERM");
  await until(exited, "the interrupted controller");
  // A display started after the cleanup began would write its line within a few milliseconds.
  await delay(300);

  assert.equal(controller.exitCode, 143, stderr);
  assert.deepEqual(started(), [xvfb], "the cleanup was followed by another Xvfb");
  assert.equal(fs.existsSync(`/proc/${xvfb}`), false, "the session's Xvfb is still running");
  assert.equal(fs.existsSync(scratch), false, "the scratch root was left behind");
  assert.equal(f.exists(gpuiControlLock), false, "the lock was left behind");
  assert.equal(manifest().status, "stopped");
});

test("compare:ui does not start next to a control session of either application", (t) => {
  for (const [lockFile, message] of [
    [appControlLock, /Refusing to compare: artifacts\/app-control\/active\.lock exists/],
    [gpuiControlLock, /Refusing to compare: artifacts\/gpui-control\/active\.lock exists/]
  ]) {
    const f = fixture(t);
    const lock = f.writeLock(lockFile);

    const result = f.compare("shell-empty");

    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, message);
    assert.equal(f.read(lockFile), lock);
    assertNothingStarted(f);
  }
});

test("unknown scenarios are refused before a session exists", (t) => {
  const f = fixture(t);

  const started = f.control("start", "--scenario", "no-such-state");
  assert.equal(started.status, 1);
  assert.match(started.stderr, /Unknown scenario no-such-state; known: .*shell-empty, shell-empty-light/);
  assert.equal(f.exists(gpuiControlLock), false);

  const compared = f.compare("shell-empty", "no-such-state");
  assert.equal(compared.status, 1);
  assert.match(compared.stderr, /Unknown scenario no-such-state/);
  assertNothingStarted(f);

  const listed = f.compare("--list");
  assert.equal(listed.status, 0, listed.stderr);
  assert.deepEqual(listed.stdout.trim().split("\n"), [
    "bulk-dock-group\ttheme=dark\tseed=control",
    "bulk-dock-tags\ttheme=dark\tseed=control",
    "gallery-default\ttheme=dark\tseed=control",
    "gallery-selection\ttheme=dark\tseed=control",
    "gallery-tiles-large\ttheme=dark\tseed=control",
    "gallery-tiles-small\ttheme=dark\tseed=control",
    "group-order-modal\ttheme=dark\tseed=control",
    "search-applied\ttheme=dark\tseed=control",
    "search-suggestions\ttheme=dark\tseed=control",
    "shell-empty\ttheme=dark\tseed=empty",
    "shell-empty-light\ttheme=light\tseed=empty",
    "tag-list-modal\ttheme=dark\tseed=control"
  ]);
});

for (const [name, from, to, message] of [
  [
    "another identifier",
    'gpuiE2eIdentifier = "com.example.mediatagger.gpui.e2e"',
    'gpuiE2eIdentifier = "com.example.mediatagger.gpui.dev"',
    /does not allow the isolated identifier|expected isolated identifier/
  ],
  [
    "another window title",
    'gpuiE2eWindowTitle = "Tagrove GPUI E2E"',
    'gpuiE2eWindowTitle = "Tagrove GPUI Dev"',
    /expected isolated window title Tagrove GPUI E2E, received Tagrove GPUI Dev/
  ],
  [
    "the development target directory",
    'path.resolve(projectRoot, "src-gpui", "target-e2e")',
    'path.resolve(projectRoot, "src-gpui", "target")',
    /unsafe target directory/
  ],
  [
    "a binary outside the E2E target",
    'path.resolve(gpuiE2eTargetDir, "debug", "media_tagger_gpui")',
    'path.resolve(projectRoot, "src-gpui", "target", "debug", "media_tagger_gpui")',
    /binary outside the E2E target/
  ]
]) {
  test(`a build script that names ${name} is refused before the lock and the build`, (t) => {
    const f = fixture(t);
    f.edit("scripts/gpui-build-e2e.mjs", from, to);

    const result = f.control("start");

    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, message);
    assert.equal(f.exists(gpuiControlLock), false);
    assertNothingStarted(f);
  });
}

test("client commands need a session id and a known session", (t) => {
  const f = fixture(t);
  for (const args of [["doctor"], ["doctor", "../other"], ["screenshot", "session"]]) {
    const result = f.control(...args);
    assert.equal(result.status, 1, args.join(" "));
    assert.match(result.stderr, /Expected a session ID from start/);
  }
  const unknown = f.control("click", id, "button");
  assert.equal(unknown.status, 1);
  assert.match(unknown.stderr, /Invalid control command/);
  const missing = f.control("stop", id);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /ENOENT/);
  assert.equal(f.control("help").status, 0);
  assertNothingStarted(f);
});
