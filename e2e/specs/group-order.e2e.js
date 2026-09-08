import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { copyPngFixtures } from "../fixtures.js";

let mediaRoot;
const output = path.resolve("artifacts/group-order");

async function invoke(command, payload = {}) {
  const response = await browser.executeAsync(
    (name, args, done) => {
      window.__TAURI_INTERNALS__.invoke(name, args).then(
        (value) => done({ value }),
        (error) => done({ error: String(error) })
      );
    },
    command,
    payload
  );
  if (response.error) throw new Error(response.error);
  return response.value;
}

async function visibleOrder() {
  return browser.execute(() =>
    [...document.querySelectorAll("[data-sort-asset]")].map((tile) =>
      Number(tile.dataset.sortAsset)
    )
  );
}

async function openSorter() {
  const trigger = await $('[data-testid="bulk-open-order-modal"]');
  await trigger.waitForEnabled();
  await trigger.click();
  await $('[data-testid="bulk-order-grid"]').waitForDisplayed();
}

async function waitForGone(selector) {
  // Query existence directly: creating a WebDriver handle races with modal unmount.
  await browser.waitUntil(
    () => browser.execute((query) => document.querySelector(query) === null, selector),
    { timeoutMsg: `Expected ${selector} to close` }
  );
}

async function persistedOrder() {
  const page = await invoke("list_assets", {
    offset: 0,
    limit: 100,
    tagsAnd: [],
    tagsNot: [],
    kind: null,
    favoritesOnly: false,
    metaFilter: { type: "groupName", groupName: "sorting-proof" }
  });
  return page.items
    .sort((a, b) => a.media_group_order - b.media_group_order)
    .map((item) => item.id);
}

async function dragWithInsertionLine(fromId, toId, side) {
  const points = await browser.execute(
    (from, to) => {
      const image = document
        .querySelector(`[data-sort-asset="${from}"] img`)
        .getBoundingClientRect();
      const target = document.querySelector(`[data-sort-asset="${to}"]`).getBoundingClientRect();
      return {
        fromX: Math.round(image.x + image.width / 2),
        fromY: Math.round(image.y + image.height / 2),
        toX: Math.round(target.x + target.width / 2),
        toY: Math.round(target.y + target.height / 2)
      };
    },
    fromId,
    toId
  );
  await browser.performActions([
    {
      id: "insertion-proof",
      type: "pointer",
      parameters: { pointerType: "mouse" },
      actions: [
        { type: "pointerMove", duration: 0, x: points.fromX, y: points.fromY },
        { type: "pointerDown", button: 0 },
        { type: "pointerMove", duration: 300, x: points.toX, y: points.toY }
      ]
    }
  ]);
  try {
    await $('[data-testid="bulk-order-insertion-line"]').waitForDisplayed();
    const correctSide = await browser.execute(
      (targetId, placement) => {
        const line = document
          .querySelector('[data-testid="bulk-order-insertion-line"]')
          .getBoundingClientRect();
        const target = document
          .querySelector(`[data-sort-asset="${targetId}"]`)
          .getBoundingClientRect();
        return (
          line.height > 180 &&
          (placement === "after" ? line.left >= target.right - 4 : line.right <= target.left + 4)
        );
      },
      toId,
      side
    );
    assert.ok(correctSide, `Insertion line should appear ${side} the target`);
    await browser.saveScreenshot(path.join(output, `insertion-${side}.png`));
    await browser.performActions([
      {
        id: "insertion-proof",
        type: "pointer",
        parameters: { pointerType: "mouse" },
        actions: [{ type: "pointerUp", button: 0 }]
      }
    ]);
  } finally {
    await browser.releaseActions();
  }
  await waitForGone('[data-testid="bulk-order-insertion-line"]');
}

describe("large group order modal", function () {
  this.timeout(90000);
  before(async () => {
    mediaRoot = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-group-order-"));
    await copyPngFixtures(mediaRoot, "sort", 24);
    await fs.mkdir(output, { recursive: true });
    await invoke("clear_library_data");
    await invoke("add_scan_root", { path: mediaRoot });
    await invoke("rescan_all_roots");
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
    await $('button[aria-label="Enable bulk actions"]').click();
    await $('button[data-asset-index="0"]').click();
    await browser.execute(() => {
      const scroller = document.querySelector(".gallery-scroll");
      scroller.scrollTop = scroller.scrollHeight;
    });
    const last = await $('button[data-asset-index="23"]');
    await last.waitForDisplayed();
    await browser.execute(
      (element) =>
        element.dispatchEvent(new MouseEvent("click", { bubbles: true, shiftKey: true })),
      last
    );
    await browser.waitUntil(async () =>
      (await $('[data-testid="bulk-header-panel"]').getText()).includes("Selected: 24")
    );
    await $("#bulk-group-key-input").setValue("sorting-proof");
  });

  afterEach(async function () {
    if (this.currentTest.state === "failed")
      await browser.saveScreenshot(path.join(output, "failure.png"));
  });

  after(async () => {
    await invoke("clear_library_data");
    if (mediaRoot) await fs.rm(mediaRoot, { recursive: true, force: true });
  });

  it("drags, cancels, saves, reopens and scrolls through the selected group", async () => {
    await openSorter();
    const initial = await visibleOrder();
    assert.ok(
      initial.length > 4 && initial.length <= 24,
      `Unexpected mounted tile count: ${initial.length}`
    );
    const first = initial[0];
    const second = initial[1];
    await dragWithInsertionLine(first, second, "after");
    await browser.waitUntil(async () => (await visibleOrder())[0] === second, {
      timeoutMsg: "pointer drop did not reorder"
    });
    await dragWithInsertionLine(first, second, "before");
    assert.deepEqual(await visibleOrder(), initial);
    assert.deepEqual(await persistedOrder(), []);
    await $("button=Cancel").click();
    await openSorter();
    assert.deepEqual(await visibleOrder(), initial);

    await $(`[data-sort-asset="${first}"] img`).dragAndDrop(
      await $(`[data-sort-asset="${second}"]`),
      {
        duration: 300
      }
    );
    await browser.waitUntil(async () => (await visibleOrder())[0] === second);
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          [...document.querySelectorAll("[data-sort-asset] img")]
            .filter((img) => img.getBoundingClientRect().bottom < innerHeight)
            .every((img) => img.complete && img.naturalWidth > 1)
        ),
      { timeout: 20000 }
    );
    await browser.saveScreenshot(path.join(output, "wide.png"));
    const previewSize = await browser.execute(
      () => document.querySelector("[data-sort-asset] img").getBoundingClientRect().height
    );
    assert.ok(previewSize >= 180, `Preview too small: ${previewSize}`);
    await $("button=Save order").click();
    await waitForGone('[data-testid="bulk-order-modal"]');
    const saved = await persistedOrder();
    assert.equal(saved.length, 24);
    assert.deepEqual(saved.slice(0, 2), [second, first]);
    await openSorter();
    assert.deepEqual((await visibleOrder()).slice(0, 2), [second, first]);

    // Hold a real pointer at the bottom edge while virtual rows scroll underneath it.
    const handle = await $(`[data-sort-handle="${second}"]`);
    const coords = await browser.execute((element) => {
      const from = element.getBoundingClientRect();
      const grid = document
        .querySelector('[data-testid="bulk-order-grid"]')
        .getBoundingClientRect();
      return {
        x: Math.round(from.x + from.width / 2),
        y: Math.round(from.y + from.height / 2),
        edgeX: Math.round(grid.left + 80),
        edgeY: Math.round(grid.bottom - 8)
      };
    }, handle);
    await browser
      .action("pointer")
      .move({ x: coords.x, y: coords.y })
      .down()
      .move({ x: coords.edgeX, y: coords.edgeY, duration: 500 })
      .pause(1400)
      .up()
      .perform();
    assert.ok(
      await browser.execute(
        () => document.querySelector('[data-testid="bulk-order-grid"]').scrollTop > 200
      )
    );
    await $("button=Save order").click();
    await waitForGone('[data-testid="bulk-order-modal"]');
    const scrolledSave = await persistedOrder();
    assert.ok(
      scrolledSave.indexOf(second) > 3,
      "edge scrolling did not move the first item to a later row"
    );

    await browser.setWindowSize(800, 700);
    await openSorter();
    await browser.saveScreenshot(path.join(output, "narrow.png"));
    assert.ok(await $("button=Save order").isDisplayed());
    assert.ok(await $("button=Cancel").isDisplayed());
    await browser.keys("Escape");
    await waitForGone('[data-testid="bulk-order-modal"]');
    await fs.writeFile(
      path.join(output, "persisted-order.json"),
      JSON.stringify({ initialSave: saved, afterEdgeDrag: scrolledSave }, null, 2)
    );
  });
});
