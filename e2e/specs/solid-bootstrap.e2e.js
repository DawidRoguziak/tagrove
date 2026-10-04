import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";

const suite = process.env.MEDIATAGGER_FRONTEND === "solid" ? describe : describe.skip;

suite("Solid desktop bootstrap", () => {
  it("mounts inside the isolated native window", async () => {
    // The harness checks the live Tauri title before running this test.
    await $("main h1").waitForDisplayed();
    assert.equal(await $("main h1").getText(), "Tagrove");
    assert.equal(await $('[role="status"]').getText(), "Solid 2 bootstrap ready");
    await browser.refresh();
    await $('[role="status"]').waitForDisplayed();
    assert.equal(await $('[role="status"]').getText(), "Solid 2 bootstrap ready");
    const evidence = path.resolve("artifacts/solid2/stage1");
    await fs.mkdir(evidence, { recursive: true });
    await browser.saveScreenshot(path.join(evidence, "solid-bootstrap.png"));
  });
});
