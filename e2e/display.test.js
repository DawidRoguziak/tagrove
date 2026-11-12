import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { assertPrivateDisplay } from "./display.js";

async function fakeProc(processes) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-display-test-"));
  for (const [pid, argv, environ = []] of processes) {
    await fs.mkdir(path.join(root, pid));
    await fs.writeFile(path.join(root, pid, "cmdline"), `${argv.join("\0")}\0`);
    await fs.writeFile(path.join(root, pid, "environ"), `${environ.join("\0")}\0`);
  }
  return root;
}

test("the capture guard accepts only the run's own private Xvfb", async () => {
  const auth = "/tmp/xvfb-run.abc/Xauthority";
  const procRoot = await fakeProc([
    ["10", ["/usr/bin/Xvfb", ":99", "-screen", "0", "1440x900x24", "-nolisten", "tcp", "-auth", auth]],
    ["11", ["Xvfb", ":0", "-auth", auth]],
    ["12", ["/usr/bin/Xorg", ":1", "-auth", auth]],
    ["13", ["Xvfb", ":0"], [`XAUTHORITY=${auth}`]],
    // Newer xvfb-run passes the authority file through Xvfb's environment.
    ["14", ["Xvfb", ":77", "-screen", "0", "1440x900x24"], ["DISPLAY=:0", `XAUTHORITY=${auth}.env`]]
  ]);
  const ok = { DISPLAY: ":99", MEDIATAGGER_NATIVE_DISPLAY: ":99", XAUTHORITY: auth };
  const options = { procRoot };
  try {
    assert.doesNotThrow(() => assertPrivateDisplay(ok, options));
    const viaEnviron = { DISPLAY: ":77", MEDIATAGGER_NATIVE_DISPLAY: ":77", XAUTHORITY: `${auth}.env` };
    assert.doesNotThrow(() => assertPrivateDisplay(viaEnviron, options));
    const refusals = [
      { ...ok, DISPLAY: undefined },
      { ...ok, DISPLAY: "remote:99" },
      { ...ok, DISPLAY: ":99.0" },
      { ...ok, MEDIATAGGER_NATIVE_DISPLAY: undefined },
      { ...ok, DISPLAY: ":98" },
      { ...ok, DISPLAY: ":0", MEDIATAGGER_NATIVE_DISPLAY: ":0" },
      { ...ok, DISPLAY: ":1", MEDIATAGGER_NATIVE_DISPLAY: ":1" },
      { ...ok, XAUTHORITY: undefined },
      { ...ok, XAUTHORITY: "Xauthority" },
      { ...ok, XAUTHORITY: "/tmp/other/Xauthority" },
      { ...ok, DISPLAY: ":42", MEDIATAGGER_NATIVE_DISPLAY: ":42" },
      { ...viaEnviron, XAUTHORITY: auth },
      { ...ok, XAUTHORITY: `${auth}.env` }
    ];
    for (const env of refusals)
      assert.throws(() => assertPrivateDisplay(env, options), /Refusing native capture/, JSON.stringify(env));
    assert.throws(() => assertPrivateDisplay(ok, { procRoot: path.join(procRoot, "missing") }), /no Xvfb/);
  } finally {
    await fs.rm(procRoot, { recursive: true, force: true });
  }
});

test("the capture guard ignores an Xvfb owned by another user", async () => {
  const auth = "/tmp/xvfb-run.abc/Xauthority";
  const procRoot = await fakeProc([["10", ["Xvfb", ":99", "-auth", auth]]]);
  const env = { DISPLAY: ":99", MEDIATAGGER_NATIVE_DISPLAY: ":99", XAUTHORITY: auth };
  try {
    const { uid } = await fs.stat(path.join(procRoot, "10"));
    assert.doesNotThrow(() => assertPrivateDisplay(env, { procRoot, uid }));
    assert.throws(() => assertPrivateDisplay(env, { procRoot, uid: uid + 1 }), /no Xvfb/);
  } finally {
    await fs.rm(procRoot, { recursive: true, force: true });
  }
});
