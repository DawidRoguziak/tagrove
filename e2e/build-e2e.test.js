import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
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
    let title = "Tagrove";
    let reloads = 0;
    globalThis.browser = {
      executeAsync: async () => ({ ok: true, value: title }),
      execute: async fn => fn(),
      refresh: async () => { reloads++; }
    };
    await assert.rejects(config.before(), /unexpected application window/);
    assert.equal(storage.size, 1);
    assert.equal(reloads, 0);
    title = "Tagrove E2E";
    await config.before();
    assert.deepEqual([...storage], [
      ["unrelated", "keep"], ["media-tagger.language", "en"], ["media-tagger.theme", "dark"]
    ]);
    assert.equal(reloads, 1);
  `]);
  assert.equal(result.status, 0, result.stderr);
});
