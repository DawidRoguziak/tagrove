import assert from "node:assert/strict";
import { constants } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";
import { e2eBinaryPath, e2eWindowTitle } from "../build-e2e.js";

// Linux only: a disposable ffprobe wrapper holds real metadata probing across reload.
const suite = process.env.MEDIATAGGER_STARTUP_REMOUNT === "1" ? describe : describe.skip;
async function invoke(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    window.__TAURI_INTERNALS__.invoke(name, args).then(
      value => done({ value }), error => done({ error: String(error) })
    );
  }, command, payload);
  if (response.error) throw new Error(response.error);
  return response.value;
}
async function ready() {
  await $(".filter-input").waitForDisplayed({ timeout: 20000 });
  assert.equal(await invoke("plugin:window|title", { label: "main" }), e2eWindowTitle);
}
async function exists(file) {
  return fs.access(file).then(() => true, () => false);
}
async function count() {
  return Number((await $(".workspace-count").getText()).match(/\d+/)?.[0]);
}

suite("startup scan completion across WebView remount", function () {
  this.timeout(90000);
  let fixture;
  let executable;
  let hold;
  before(async () => {
    await ready();
    fixture = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-startup-remount-"));
    const app = path.join(fixture, "app");
    await fs.mkdir(app);
    executable = path.join(app, "media_tagger");
    // Keep the guarded E2E build intact. Only this temporary executable sees the wrapper.
    await fs.copyFile(e2eBinaryPath, executable, constants.COPYFILE_FICLONE);
    await fs.chmod(executable, 0o755);
    await fs.symlink("/usr/bin/ffmpeg", path.join(app, "ffmpeg"));
    hold = path.join(app, "hold");
    await fs.writeFile(path.join(app, "ffprobe"), `#!/usr/bin/python3
import pathlib, subprocess, sys, time
base = pathlib.Path(__file__).parent
if '-show_entries' in sys.argv and (base / 'hold').exists():
    with (base / 'entered').open('a') as marker:
        marker.write('probe\\n')
    deadline = time.monotonic() + 10
    while (base / 'hold').exists():
        if time.monotonic() > deadline:
            (base / 'expired').touch()
            sys.exit(1)
        time.sleep(0.01)
sys.exit(subprocess.call(['/usr/bin/ffprobe', *sys.argv[1:]]))
`, { mode: 0o755 });
  });
  after(async () => {
    if (!fixture) return;
    if (hold) await fs.rm(hold, { force: true });
    await ready();
    await invoke("scan_startup_roots");
    await invoke("clear_library_data");
    await browser.reloadSession({ "tauri:options": { application: e2eBinaryPath } });
    await ready();
    await fs.rm(fixture, { recursive: true, force: true });
  });

  for (const initial of [0, 2]) {
    it(`refreshes ${initial} cached rows after a reload during native scanning`, async () => {
      await ready();
      await invoke("scan_startup_roots");
      await invoke("clear_library_data");
      const media = path.join(fixture, `media-${initial}`);
      await fs.mkdir(media);
      await copyPngFixtures(media, "cached", initial);
      await invoke("scan_folder", { path: media });
      await invoke("set_scan_root_auto_scan", { path: media, enabled: true });
      const generated = path.join(fixture, `generated-${initial}`);
      await fs.mkdir(generated);
      const { videoPaths } = await createPlayableFixtures(generated, "new");
      await fs.copyFile(videoPaths[0], path.join(media, "new.mp4"));
      const entered = path.join(path.dirname(executable), "entered");
      const expired = path.join(path.dirname(executable), "expired");
      await fs.rm(entered, { force: true });
      await fs.rm(expired, { force: true });
      await fs.writeFile(hold, "hold native scan");
      await browser.reloadSession({ "tauri:options": { application: executable } });
      await ready();
      await browser.waitUntil(() => exists(entered), { timeout: 5000 });
      await browser.waitUntil(async () => await count() === initial, { timeout: 5000 });
      await browser.refresh();
      await ready();
      await browser.waitUntil(async () => await count() === initial, { timeout: 5000 });
      assert.equal(await exists(expired), false, "probe must still be held across reload");
      await fs.rm(hold);
      // No navigation, filter change, manual refresh or page request to recover the UI.
      await browser.waitUntil(async () => await count() === initial + 1, {
        timeout: 15000, timeoutMsg: "Replacement gallery missed startup completion"
      });
      const summary = await invoke("scan_startup_roots");
      assert.equal(summary.indexed, initial + 1);
      assert.equal(await exists(expired), false);
      assert.equal((await fs.readFile(entered, "utf8")).trim(), "probe");
      const evidence = path.resolve("artifacts/startup-scan-remount");
      await fs.mkdir(evidence, { recursive: true });
      await browser.saveScreenshot(path.join(evidence, `remount-${initial}.png`));
      await fs.writeFile(path.join(evidence, `remount-${initial}.json`), JSON.stringify({
        initial, final: await count(), summary, probeRuns: 1, expired: false
      }, null, 2));
      await browser.refresh();
      await ready();
      await browser.waitUntil(async () => await count() === initial + 1);
      assert.deepEqual(await invoke("scan_startup_roots"), summary);
      assert.equal((await fs.readFile(entered, "utf8")).trim(), "probe");
    });
  }
});
