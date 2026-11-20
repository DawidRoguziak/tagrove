import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import {
  buildResult,
  parseCompareArguments,
  parseControllerLine,
  portableError,
  resetScenarioOutput,
  scenarioOutputDirectory,
  settlesWithin
} from "./compare-support.js";

const id = "a".repeat(32);
const signature = "0123456789abcdef".repeat(4);
const scenario = { id: "shell-empty", theme: "dark", seed: "empty", tauri: [] };
const screen = { width: 1440, height: 900, depth: 24 };
const capture = (title) => ({ ok: true, title, signature, width: 1440, height: 900 });

test("takes scenario ids, each once, or --list", () => {
  assert.deepEqual(parseCompareArguments(["--list"]), { list: true, scenarios: [] });
  assert.deepEqual(parseCompareArguments(["shell-empty", "shell-empty-light"]), {
    list: false,
    scenarios: ["shell-empty", "shell-empty-light"]
  });
  assert.throws(() => parseCompareArguments([]), /at least one scenario/);
  assert.throws(() => parseCompareArguments(["shell-empty", "shell-empty"]), /named twice/);
  assert.throws(() => parseCompareArguments(["--scenario", "shell-empty"]), /Unknown option --scenario/);
  assert.throws(() => parseCompareArguments(["--list", "shell-empty"]), /Unknown option --list/);
  assert.throws(() => parseCompareArguments(["../../etc"]), /scenario id/);
});

test("output goes to artifacts/gpui-compare/<scenario> and nowhere else", () => {
  assert.equal(
    scenarioOutputDirectory("/synthetic/checkout", "shell-empty"),
    "/synthetic/checkout/artifacts/gpui-compare/shell-empty"
  );
  for (const invalid of ["", ".", "..", "../app-control", "a/b", "Shell"])
    assert.throws(() => scenarioOutputDirectory("/synthetic/checkout", invalid), /scenario id/, invalid);
});

test("a run starts from an empty scenario directory and leaves the others alone", async (t) => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-compare-output-test-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const first = await resetScenarioOutput(root, "shell-empty");
  const other = await resetScenarioOutput(root, "shell-empty-light");
  await fs.writeFile(path.join(first, "tauri-failed.png"), "stale");
  await fs.writeFile(path.join(other, "tauri.png"), "kept");

  assert.equal(await resetScenarioOutput(root, "shell-empty"), first);

  assert.deepEqual(await fs.readdir(first), []);
  assert.equal(await fs.readFile(path.join(other, "tauri.png"), "utf8"), "kept");
});

test("reads the session from a controller's BUILD and READY lines", () => {
  assert.deepEqual(parseControllerLine(`BUILD ${id}; output: /synthetic/process.log`), { type: "build", id });
  assert.deepEqual(parseControllerLine(`READY {"id":"${id}","title":"Tagrove E2E"}`), {
    type: "ready",
    id,
    value: { id, title: "Tagrove E2E" }
  });
  for (const line of ["", "READY", "READY {}", 'READY {"id":"../x"}', "READY {broken", `BUILD ${id}`, "building"])
    assert.equal(parseControllerLine(line), null, line);
});

test("a wait reports whether the promise settled in time", async () => {
  assert.equal(await settlesWithin(Promise.resolve("exit"), 60_000), true);
  assert.equal(await settlesWithin(new Promise(() => {}), 10), false);
  await assert.rejects(settlesWithin(Promise.reject(new Error("broken")), 60_000), /broken/);
});

test("a wait the promise won leaves no timer that keeps the process alive", () => {
  // compare:ui ends by returning from main(), so a pending stop timeout would hold the
  // process for its whole length after the results are written.
  const support = fileURLToPath(new URL("./compare-support.js", import.meta.url));
  const script = `
    const { settlesWithin } = await import(process.argv[1]);
    console.log(await settlesWithin(Promise.resolve(), 10 * 60_000));
  `;
  const started = Date.now();
  const child = spawnSync(process.execPath, ["--input-type=module", "-e", script, support], {
    encoding: "utf8",
    timeout: 30_000,
    env: { ...process.env, NODE_TEST_CONTEXT: undefined }
  });
  assert.equal(child.signal, null, "the process waited for the lost timeout");
  assert.equal(child.status, 0, child.stderr);
  assert.equal(child.stdout.trim(), "true");
  assert.ok(Date.now() - started < 30_000);
});

test("result.json names both captures and holds no time or absolute path", () => {
  const result = buildResult({
    scenario,
    description: "Empty library",
    screen,
    tauri: capture("Tagrove E2E"),
    gpui: capture("Tagrove GPUI E2E"),
    sideBySide: true
  });
  assert.deepEqual(result, {
    schema: 1,
    scenario: "shell-empty",
    description: "Empty library",
    theme: "dark",
    seed: "empty",
    screen: { width: 1440, height: 900 },
    ok: true,
    tauri: { ok: true, file: "tauri.png", width: 1440, height: 900, signature, title: "Tagrove E2E" },
    gpui: { ok: true, file: "gpui.png", width: 1440, height: 900, signature, title: "Tagrove GPUI E2E" },
    sideBySide: "side-by-side.png"
  });
  assert.doesNotMatch(JSON.stringify(result), /\/|\d{4}-\d{2}-\d{2}/);
});

test("a failed side fails the scenario and keeps the other capture", () => {
  const result = buildResult({
    scenario,
    screen,
    tauri: { ok: false, error: "tauri click failed" },
    gpui: capture("Tagrove GPUI E2E"),
    sideBySide: false
  });
  assert.equal(result.ok, false);
  assert.equal(result.description, null);
  assert.deepEqual(result.tauri, { ok: false, error: "tauri click failed" });
  assert.equal(result.gpui.ok, true);
  assert.equal(result.sideBySide, null);

  const missing = buildResult({ scenario, screen, tauri: capture("Tagrove E2E"), gpui: undefined, sideBySide: false });
  assert.deepEqual(missing.gpui, { ok: false, error: "not captured" });
  assert.equal(missing.ok, false);
});

const places = [
  ["/synthetic/home/checkout", "<checkout>"],
  ["/synthetic/tmp/", "<tmp>"],
  ["/synthetic/home", "<home>"]
];

test("an error names the known directories and quotes no absolute path", () => {
  const session = `/synthetic/home/checkout/artifacts/gpui-control/${id}`;
  assert.equal(
    portableError(`The application exited during startup (1); see ${session}/process.log`, places),
    `The application exited during startup (1); see <checkout>/artifacts/gpui-control/${id}/process.log`
  );
  assert.equal(
    portableError("The application did not open its profile in /synthetic/tmp/scratch-Ab12/data", places),
    "The application did not open its profile in <tmp>/scratch-Ab12/data"
  );
  assert.equal(
    portableError("at start (file:///synthetic/home/checkout/e2e/control.js:12:3)", places),
    "at start (<checkout>/e2e/control.js:12:3)"
  );
  assert.equal(
    portableError("spawn /synthetic/home/.cargo/bin/tauri-driver ENOENT, open '/usr/lib/x.so'", places),
    "spawn <home>/.cargo/bin/tauri-driver ENOENT, open '<path>'"
  );
  // A directory is replaced only as a whole path component, the longest one first.
  assert.equal(
    portableError("/synthetic/home/checkout-2/a /synthetic/home/checkout: /synthetic/homework", places),
    "<home>/checkout-2/a <checkout>: <path>"
  );
  // What is not an absolute path stays: relative paths, URLs, sizes and a lone slash.
  const kept = "tauri click failed: e2e/x.js ./a ../b ~/c http://127.0.0.1:4444/session 1440x900 a / b";
  assert.equal(portableError(kept, places), kept);
  // Without known directories every absolute path still goes, and "/" is never a directory.
  assert.equal(portableError("see /synthetic/home/checkout/process.log"), "see <path>");
  const unusable = [
    ["/", "<root>"],
    ["relative", "<relative>"],
    [undefined, "<unset>"]
  ];
  assert.equal(portableError("see /synthetic/x.log", unusable), "see <path>");
  assert.equal(portableError(new Error("broken").message), "broken");
});

test("the error of a failed side reaches result.json without an absolute path", () => {
  const evidence = "/synthetic/home/checkout/artifacts";
  const result = buildResult({
    scenario,
    screen,
    tauri: {
      ok: false,
      error: `tauri start ended before READY (1): see ${evidence}/app-control/${id}/process.log`
    },
    gpui: {
      ok: false,
      error: `gpui scenario shell-empty failed: see ${evidence}/gpui-control/${id}/process.log`
    },
    sideBySide: false,
    places
  });
  assert.deepEqual(result.tauri, {
    ok: false,
    error: `tauri start ended before READY (1): see <checkout>/artifacts/app-control/${id}/process.log`
  });
  assert.deepEqual(result.gpui, {
    ok: false,
    error: `gpui scenario shell-empty failed: see <checkout>/artifacts/gpui-control/${id}/process.log`
  });
  assert.doesNotMatch(JSON.stringify(result), /synthetic|(?<![\w>])\//);
});

test("a capture of another size fails the scenario", () => {
  const result = buildResult({
    scenario,
    screen,
    tauri: capture("Tagrove E2E"),
    gpui: { ...capture("Tagrove GPUI E2E"), height: 1100 },
    sideBySide: true
  });
  assert.equal(result.ok, false);
  assert.equal(result.gpui.ok, false);
  assert.match(result.gpui.error, /1440x1100, expected 1440x900/);
});
