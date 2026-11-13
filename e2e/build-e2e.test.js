import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { test } from "node:test";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mediatagger-build-test-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const file of ["e2e/build-e2e.js", "e2e/wdio.conf.js", "src-tauri/tauri.conf.e2e.json"]) {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(file, target);
  }
  fs.writeFileSync(path.join(root, "package.json"), '{"type":"module"}');
  fs.mkdirSync(path.join(root, "bin"));
  const command = (name, script) => fs.writeFileSync(path.join(root, "bin", name), `#!/bin/sh\nset -eu\n${script}\n`, { mode: 0o755 });
  command("bun", 'printf "frontend\\n" >> calls\nexit "$FAIL_FRONTEND"');
  command("tauri", 'printf "native\\n" >> calls\nmkdir -p "$CARGO_TARGET_DIR/debug"\ntouch "$CARGO_TARGET_DIR/debug/media_tagger"');
  const run = (env, args = ["e2e/build-e2e.js"]) => spawnSync(process.execPath, args, {
    cwd: root, encoding: "utf8",
    env: { ...process.env, NODE_TEST_CONTEXT: undefined, FAIL_FRONTEND: "0", XDG_DATA_HOME: path.join(root, "data"),
      PATH: `${path.join(root, "bin")}:${process.env.PATH}`, ...env }
  });
  const calls = () => fs.existsSync(path.join(root, "calls")) ? fs.readFileSync(path.join(root, "calls"), "utf8") : "";
  return { root, run, calls };
}

test("desktop builds the normal frontend before native packaging", t => {
  const { run, calls } = fixture(t);
  assert.equal(run({}).status, 0);
  assert.equal(calls(), "frontend\nnative\n");
});

test("unsafe profiles are rejected before running build tools", t => {
  const { root, run, calls } = fixture(t);
  const config = path.join(root, "src-tauri/tauri.conf.e2e.json");
  const data = JSON.parse(fs.readFileSync(config, "utf8"));
  data.identifier = "com.example.mediatagger";
  fs.writeFileSync(config, JSON.stringify(data));
  assert.notEqual(run({}).status, 0);
  assert.equal(calls(), "");
});

test("WDIO preparation exits on frontend failure before workers can launch", t => {
  const { run, calls } = fixture(t);
  const result = run({ FAIL_FRONTEND: "23" }, ["--input-type=module", "-e",
    'import { config } from "./e2e/wdio.conf.js"; config.onPrepare(); console.log("UNSAFE_CONTINUATION");']);
  assert.equal(result.status, 1);
  assert.doesNotMatch(result.stdout, /UNSAFE_CONTINUATION/);
  assert.equal(calls(), "frontend\n");
});

test("spec preferences reset only after the native title guard, without clearing other storage", t => {
  const { run } = fixture(t);
  const result = run({}, ["--input-type=module", "-e", `
    import assert from "node:assert/strict";
    import { config } from "./e2e/wdio.conf.js";
    const storage = new Map([["unrelated", "keep"]]);
    globalThis.localStorage = { setItem: (key, value) => storage.set(key, value) };
    let reloads = 0;
    globalThis.browser = {
      executeAsync: async () => ({ ok: true, value: "Tagrove E2E" }),
      execute: async fn => fn(),
      refresh: async () => { reloads++; }
    };
    await config.before();
    assert.deepEqual([...storage], [
      ["unrelated", "keep"], ["media-tagger.language", "en"], ["media-tagger.theme", "dark"]
    ]);
    assert.equal(reloads, 1);
  `]);
  assert.equal(result.status, 0, result.stderr);
});

for (const [name, executeAsync] of [
  ["an unexpected live title", `async () => ({ ok: true, value: "Tagrove" })`],
  ["an unreadable live title", `async () => ({ ok: false, error: "no bridge" })`]
]) {
  test(`${name} ends the worker with the guard-abort status before any preference write`, t => {
    const { run } = fixture(t);
    // WDIO swallows a throwing before hook, so the guard must exit the process itself.
    const result = run({}, ["--input-type=module", "-e", `
      import { config } from "./e2e/wdio.conf.js";
      globalThis.localStorage = { setItem: () => console.log("PREFERENCE_WRITE") };
      globalThis.browser = {
        executeAsync: ${executeAsync},
        execute: async fn => fn(),
        refresh: async () => console.log("RELOAD")
      };
      await config.before();
      console.log("UNSAFE_CONTINUATION");
    `]);
    assert.equal(result.status, 78);
    assert.doesNotMatch(result.stdout, /UNSAFE_CONTINUATION|PREFERENCE_WRITE|RELOAD/);
    assert.match(result.stderr, /Refusing destructive E2E suite/);
  });
}

// Starts a child and resolves once it prints READY. Fails with the captured output when the child
// exits first or never prints READY within the bound; the child is always killed on failure.
const READY_TIMEOUT_MS = 15_000;
async function spawnReady(root, script, { ipc = false, timeoutMs = READY_TIMEOUT_MS } = {}) {
  const child = spawn(process.execPath, ["--input-type=module", "-e", script], {
    cwd: root, env: { ...process.env, NODE_TEST_CONTEXT: undefined, XDG_DATA_HOME: path.join(root, "data") },
    stdio: ["ignore", "pipe", "pipe", ...(ipc ? ["ipc"] : [])]
  });
  let stdout = "";
  let stderr = "";
  child.stderr.on("data", chunk => { stderr += chunk; });
  let timer;
  const outcome = await new Promise(resolve => {
    child.stdout.on("data", chunk => { stdout += chunk; if (stdout.includes("READY")) resolve("ready"); });
    child.once("close", (status, received) => resolve(`exited early (status ${status}, signal ${received})`));
    timer = setTimeout(() => resolve(`no READY within ${timeoutMs} ms`), timeoutMs);
  });
  clearTimeout(timer);
  if (outcome !== "ready") {
    child.kill("SIGKILL");
    assert.fail(`child ${outcome}\nstdout: ${stdout}\nstderr: ${stderr}`);
  }
  return child;
}

for (const [signal, code] of [["SIGINT", 130], ["SIGTERM", 143], ["SIGHUP", 129]]) {
  test(`${signal} ends the launcher with exit code ${code}`, async t => {
    const { root } = fixture(t);
    const child = await spawnReady(root,
      'process.on("SIGINT", () => process.exit(1)); import("./e2e/wdio.conf.js").then(() => { console.log("READY"); setInterval(() => {}, 1000); });');
    const exited = new Promise(resolve => child.once("exit", (status, received) => resolve({ status, received })));
    child.kill(signal);
    assert.deepEqual(await exited, { status: code, received: null });
  });
}

test("a worker stops when its launcher disconnects", async t => {
  const { root } = fixture(t);
  const child = await spawnReady(root,
    'import "./e2e/wdio.conf.js"; console.log("READY"); setInterval(() => {}, 1000);', { ipc: true });
  const exited = new Promise(resolve => child.once("exit", (status, received) => resolve({ status, received })));
  child.disconnect();
  assert.deepEqual(await exited, { status: 1, received: null });
});

test("the launcher ends the whole run when a worker stopped on the title guard", t => {
  const { run } = fixture(t);
  const result = run({}, ["--input-type=module", "-e", `
    import { config } from "./e2e/wdio.conf.js";
    config.onWorkerEnd("0-0", 1);
    config.onWorkerEnd("0-0", 0);
    console.log("OTHER_EXIT_CODES_CONTINUE");
    config.onWorkerEnd("0-0", 78);
    console.log("UNSAFE_CONTINUATION");
  `]);
  assert.equal(result.status, 1);
  assert.match(result.stdout, /OTHER_EXIT_CODES_CONTINUE/);
  assert.doesNotMatch(result.stdout, /UNSAFE_CONTINUATION/);
});

test("a child that never prints READY fails within the bound with its output", async t => {
  const { root } = fixture(t);
  const started = Date.now();
  await assert.rejects(
    spawnReady(root, 'console.error("hung-import"); setInterval(() => {}, 1000);', { timeoutMs: 2000 }),
    error => /no READY within 2000 ms/.test(error.message) && /hung-import/.test(error.message));
  assert.ok(Date.now() - started < 5000);
});

test("a child that exits before READY fails immediately with its stderr", async t => {
  const { root } = fixture(t);
  const started = Date.now();
  await assert.rejects(
    spawnReady(root, 'import("./missing-config.js")', { timeoutMs: 60_000 }),
    error => /exited early/.test(error.message) && /missing-config/.test(error.message));
  assert.ok(Date.now() - started < 10_000);
});
