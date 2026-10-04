import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mediatagger-build-test-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const file of ["e2e/build-e2e.js", "e2e/wdio.conf.js", "scripts/frontend.mjs", "src-tauri/tauri.conf.e2e.json"]) {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(file, target);
  }
  fs.writeFileSync(path.join(root, "package.json"), '{"type":"module"}');
  fs.mkdirSync(path.join(root, "bin"));
  const command = (name, script) => fs.writeFileSync(path.join(root, "bin", name), `#!/bin/sh\nset -eu\n${script}\n`, { mode: 0o755 });
  command("bun", 'printf "frontend:%s\\n" "$MEDIATAGGER_FRONTEND" >> calls\nexit "$FAIL_FRONTEND"');
  command("tauri", 'printf "native\\n" >> calls\nmkdir -p "$CARGO_TARGET_DIR/debug"\ntouch "$CARGO_TARGET_DIR/debug/media_tagger"');
  const run = (env, args = ["e2e/build-e2e.js"]) => spawnSync(process.execPath, args, {
    cwd: root, encoding: "utf8",
    env: { ...process.env, NODE_TEST_CONTEXT: undefined, FAIL_FRONTEND: "0", MEDIATAGGER_FRONTEND: "react", XDG_DATA_HOME: path.join(root, "data"),
      PATH: `${path.join(root, "bin")}:${process.env.PATH}`, ...env }
  });
  const calls = () => fs.existsSync(path.join(root, "calls")) ? fs.readFileSync(path.join(root, "calls"), "utf8") : "";
  return { root, run, calls };
}

test("each desktop build builds its selected frontend before native packaging", t => {
  const { run, calls } = fixture(t);
  assert.equal(run({ MEDIATAGGER_FRONTEND: "react" }).status, 0);
  assert.equal(run({ MEDIATAGGER_FRONTEND: "solid" }).status, 0);
  assert.equal(calls(), "frontend:react\nnative\nfrontend:solid\nnative\n");
});

test("invalid selection and profile are rejected before running build tools", t => {
  const { root, run, calls } = fixture(t);
  assert.notEqual(run({ MEDIATAGGER_FRONTEND: "invalid" }).status, 0);
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
  assert.equal(calls(), "frontend:react\n");
});
