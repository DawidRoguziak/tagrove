import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

async function fixture(t, running = false) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "tagrove-install-"));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const scripts = path.join(root, "scripts");
  const artifacts = path.join(root, "artifacts/linux");
  const commands = path.join(root, "commands");
  const prefix = path.join(root, "install with spaces");
  for (const dir of [scripts, artifacts, commands]) await fs.mkdir(dir, { recursive: true });
  const installer = path.join(scripts, "reinstall-linux-user.sh");
  await fs.copyFile(new URL("./reinstall-linux-user.sh", import.meta.url), installer);
  await fs.writeFile(
    path.join(commands, "pgrep"),
    `#!/bin/sh
test "$1" = "-x" && test "$2" = "media_tagger" || exit 2
exit ${running ? 0 : 1}
`,
    { mode: 0o755 }
  );
  const files = new Map([
    ["media_tagger", Buffer.from("#!/bin/sh\necho Tagrove-test-fixture\n")],
    ["tagrove.png", await fs.readFile(new URL("../src-tauri/icons/512x512.png", import.meta.url))],
    [
      "com.example.mediatagger.desktop",
      await fs.readFile(
        new URL("../src-tauri/linux/com.example.mediatagger.desktop", import.meta.url)
      )
    ],
    ["build-manifest.txt", Buffer.from("fixture=true\n")]
  ]);
  const checksums = [];
  for (const [name, data] of files) {
    await fs.writeFile(path.join(artifacts, name), data);
    checksums.push(`${createHash("sha256").update(data).digest("hex")}  ${name}`);
  }
  await fs.writeFile(path.join(artifacts, "SHA256SUMS"), `${checksums.join("\n")}\n`);
  return {
    artifacts,
    prefix,
    files,
    run: () =>
      spawnSync("bash", [installer, prefix], {
        encoding: "utf8",
        env: { ...process.env, PATH: `${commands}:${process.env.PATH}` }
      })
  };
}

test("installs and replaces the executable, Woven T icon, and matching Tagrove launcher", async (t) => {
  const f = await fixture(t);
  for (let install = 0; install < 2; install++) {
    const result = f.run();
    assert.equal(result.status, 0, result.stderr);
  }
  const executable = path.join(f.prefix, "bin/media_tagger");
  assert.equal(
    spawnSync(executable, [], { encoding: "utf8" }).stdout.trim(),
    "Tagrove-test-fixture"
  );
  assert.deepEqual(await fs.readdir(path.join(f.prefix, "bin")), ["media_tagger"]);
  const icon = await fs.readFile(
    path.join(f.prefix, "share/icons/hicolor/512x512/apps/tagrove.png")
  );
  assert.deepEqual(icon, f.files.get("tagrove.png"));
  const desktop = await fs.readFile(
    path.join(f.prefix, "share/applications/com.example.mediatagger.desktop"),
    "utf8"
  );
  assert.match(desktop, /^Name=Tagrove$/m);
  assert.match(desktop, /^Exec=media_tagger$/m);
  assert.match(desktop, /^Icon=tagrove$/m);
  assert.match(desktop, /^StartupWMClass=media_tagger$/m);
});

test("checksum failure preserves an existing installation", async (t) => {
  const f = await fixture(t);
  assert.equal(f.run().status, 0);
  const executable = path.join(f.prefix, "bin/media_tagger");
  await fs.writeFile(executable, "existing installation");
  await fs.appendFile(path.join(f.artifacts, "tagrove.png"), "corrupt");
  assert.notEqual(f.run().status, 0);
  assert.equal(await fs.readFile(executable, "utf8"), "existing installation");
});

test("a running app prevents all installation writes", async (t) => {
  const f = await fixture(t, true);
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Close Tagrove/);
  await assert.rejects(fs.stat(f.prefix), { code: "ENOENT" });
});
