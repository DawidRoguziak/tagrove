import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const profileFile = "src-gpui/crates/media_tagger_gpui/src/profile.rs";
const e2eIdentifier = "com.example.mediatagger.gpui.e2e";

// A copy of the build script and profile.rs in a scratch root, with a fake `cargo` that
// records its call and writes a binary answering `--identifier`.
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "mediatagger-gpui-build-test-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const file of ["scripts/gpui-build-e2e.mjs", profileFile]) {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(repositoryRoot, file), target);
  }
  fs.mkdirSync(path.join(root, "bin"));
  fs.writeFileSync(
    path.join(root, "bin", "cargo"),
    [
      "#!/bin/sh",
      "set -eu",
      'printf "%s\\n" "$*" >> calls',
      'printf "%s\\n" "$CARGO_TARGET_DIR" >> targets',
      '[ "$FAIL_CARGO" = 0 ] || exit "$FAIL_CARGO"',
      '[ "$SKIP_BINARY" = 0 ] || exit 0',
      'mkdir -p "$CARGO_TARGET_DIR/debug"',
      'identifier="$MEDIATAGGER_GPUI_IDENTIFIER"',
      '[ -z "$FAKE_IDENTIFIER" ] || identifier="$FAKE_IDENTIFIER"',
      'printf "#!/bin/sh\\necho %s\\n" "$identifier" > "$CARGO_TARGET_DIR/debug/media_tagger_gpui"',
      'chmod +x "$CARGO_TARGET_DIR/debug/media_tagger_gpui"',
      ""
    ].join("\n"),
    { mode: 0o755 }
  );
  const run = (env = {}) =>
    spawnSync(process.execPath, ["scripts/gpui-build-e2e.mjs"], {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        NODE_TEST_CONTEXT: undefined,
        FAIL_CARGO: "0",
        SKIP_BINARY: "0",
        FAKE_IDENTIFIER: "",
        MEDIATAGGER_GPUI_IDENTIFIER: "com.example.mediatagger.gpui.dev",
        PATH: `${path.join(root, "bin")}:${process.env.PATH}`,
        ...env
      }
    });
  const read = (name) =>
    fs.existsSync(path.join(root, name)) ? fs.readFileSync(path.join(root, name), "utf8") : "";
  const binary = path.join(root, "src-gpui", "target-e2e", "debug", "media_tagger_gpui");
  return { root, run, read, binary };
}

test("builds the locked adapter for the E2E identifier in its own target directory", (t) => {
  const { root, run, read, binary } = fixture(t);

  const result = run();

  assert.equal(result.status, 0, result.stderr);
  assert.equal(
    read("calls"),
    `build --manifest-path ${path.join(root, "src-gpui", "Cargo.toml")} --locked -p media_tagger_gpui\n`
  );
  assert.equal(read("targets"), `${path.join(root, "src-gpui", "target-e2e")}\n`);
  assert.equal(
    spawnSync(binary, ["--identifier"], { encoding: "utf8" }).stdout,
    `${e2eIdentifier}\n`
  );
  assert.equal(fs.existsSync(path.join(root, "src-gpui", "target")), false);
});

test("a profile list without the E2E profile is rejected before cargo runs", (t) => {
  for (const removed of [e2eIdentifier, "Tagrove GPUI E2E"]) {
    const { root, run, read } = fixture(t);
    const profile = path.join(root, profileFile);
    fs.writeFileSync(profile, fs.readFileSync(profile, "utf8").replaceAll(removed, "removed"));

    const result = run();

    assert.notEqual(result.status, 0, removed);
    assert.match(result.stderr, /Refusing GPUI E2E build/);
    assert.equal(read("calls"), "");
  }
});

test("a binary built for another identifier is deleted and reported", (t) => {
  const { run, binary } = fixture(t);

  const result = run({ FAKE_IDENTIFIER: "com.example.mediatagger.gpui.dev" });

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /instead of com\.example\.mediatagger\.gpui\.e2e/);
  assert.equal(fs.existsSync(binary), false);
});

test("a failed or empty cargo build is an error and leaves no stale binary", (t) => {
  const { run, binary } = fixture(t);
  assert.equal(run().status, 0);
  assert.equal(fs.existsSync(binary), true);

  const failed = run({ FAIL_CARGO: "101" });
  assert.notEqual(failed.status, 0);
  assert.match(failed.stderr, /GPUI E2E build failed/);
  assert.equal(fs.existsSync(binary), false, "the binary of the previous build is removed first");

  const empty = run({ SKIP_BINARY: "1" });
  assert.notEqual(empty.status, 0);
  assert.match(empty.stderr, /did not create the isolated binary/);
});
