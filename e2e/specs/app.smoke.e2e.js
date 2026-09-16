import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const iconsRoot = path.resolve(__dirname, "..", "..", "src-tauri", "icons");
let mediaSeeded = false;

async function invokeTauriCommand(command, payload = {}) {
  const response = await browser.executeAsync((name, args, done) => {
    const tauri =
      window.__TAURI__?.core && typeof window.__TAURI__.core.invoke === "function"
        ? window.__TAURI__.core
        : window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function"
          ? window.__TAURI_INTERNALS__
          : null;

    if (!tauri) {
      done({ ok: false, error: "Tauri invoke bridge is not available in test runtime." });
      return;
    }

    Promise.resolve()
      .then(() => tauri.invoke(name, args))
      .then((value) => done({ ok: true, value }))
      .catch((error) => done({ ok: false, error: String(error) }));
  }, command, payload);

  if (!response?.ok) {
    throw new Error(response?.error ?? `Failed to invoke command: ${command}`);
  }

  return response.value;
}

async function ensureMediaIndexed() {
  if (mediaSeeded) {
    return;
  }

  await invokeTauriCommand("add_scan_root", { path: iconsRoot });
  await invokeTauriCommand("rescan_all_roots");
  await browser.waitUntil(
    async () => {
      const page = await invokeTauriCommand("list_assets", {
        offset: 0,
        limit: 2,
        tagsAnd: [],
        tagsNot: [],
        kind: null,
        favoritesOnly: false,
        metaFilter: null
      });
      return page?.total >= 2;
    },
    {
      timeout: 15000,
      timeoutMsg: "expected at least 2 indexed media assets"
    }
  );
  mediaSeeded = true;
  await browser.refresh();
}

async function ensureGalleryView() {
  const backButton = await $('button[aria-label="Back"]');
  if (await backButton.isExisting()) {
    await backButton.waitForDisplayed({ timeout: 10000 });
    await backButton.click();
    await backButton.waitForDisplayed({ timeout: 10000, reverse: true });
  }

  const searchInput = await $(".filter-input");
  await searchInput.waitForDisplayed({ timeout: 20000 });
  return searchInput;
}

async function clearSearchFilter() {
  await ensureGalleryView();
  const clearFiltersButton = await $('button[aria-label="Clear all search filters"]');
  await clearFiltersButton.waitForDisplayed({ timeout: 10000 });
  await clearFiltersButton.click();
}

async function ensureSelectionModeEnabled() {
  const disableSelectionButton = await $('button[aria-label="Disable bulk actions"]');
  if (await disableSelectionButton.isExisting()) {
    await disableSelectionButton.waitForDisplayed({ timeout: 10000 });
    return;
  }

  const enableSelectionButton = await $('button[aria-label="Enable bulk actions"]');
  await enableSelectionButton.waitForDisplayed({ timeout: 10000 });
  await enableSelectionButton.click();
  await disableSelectionButton.waitForDisplayed({ timeout: 10000 });
}

async function selectVisibleAssets(count) {
  await browser.waitUntil(
    async () => (await $$('[data-testid="gallery-grid"] button[data-asset-id]')).length >= count,
    {
      timeout: 15000,
      timeoutMsg: `expected at least ${count} visible gallery assets`
    }
  );
  const tiles = await $$('[data-testid="gallery-grid"] button[data-asset-id]');
  for (const tile of tiles.slice(0, count)) {
    await browser.execute((element) => {
      element.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true }));
    }, tile);
  }
}

async function ensureSettingsView() {
  const backButton = await $('button[aria-label="Back"]');
  if (await backButton.isExisting()) {
    await backButton.waitForDisplayed({ timeout: 10000 });
    return;
  }

  const settingsToggle = await $('button[aria-label="Open settings"]');
  await settingsToggle.waitForDisplayed({ timeout: 10000 });
  await settingsToggle.click();
  await backButton.waitForDisplayed({ timeout: 10000 });
}

describe("MediaTagger desktop smoke", () => {
  it("opens the app and switches to settings", async () => {
    const searchInput = await ensureGalleryView();
    await ensureSettingsView();

    const appearanceHeading = await $("h2=Appearance");
    await appearanceHeading.waitForDisplayed({ timeout: 10000 });

    const scanHeading = await $("h2=Scan settings");
    await scanHeading.waitForDisplayed({ timeout: 10000 });

    const backButton = await $('button[aria-label="Back"]');
    await backButton.waitForDisplayed({ timeout: 10000 });
    await backButton.click();

    await searchInput.waitForDisplayed({ timeout: 10000 });
  });

  it("shows duplicate and danger controls in settings", async () => {
    await ensureSettingsView();

    const duplicateHeading = await $("h3=Duplicate manager");
    await duplicateHeading.waitForDisplayed({ timeout: 10000 });

    const startButton = await $("button=Start");
    await startButton.waitForDisplayed({ timeout: 10000 });

    const dangerButton = await $("button=Remove all thumbnails, indexed assets and scan paths");
    await dangerButton.waitForDisplayed({ timeout: 10000 });
    await dangerButton.click();

    const confirmDialogTitle = await $("h3=Delete all library data?");
    await confirmDialogTitle.waitForDisplayed({ timeout: 10000 });

    const cancelDialogButton = await $("button=No");
    await cancelDialogButton.waitForDisplayed({ timeout: 10000 });
    await cancelDialogButton.click();

    await confirmDialogTitle.waitForDisplayed({ timeout: 10000, reverse: true });
  });

  it("adds a bulk tag to selected assets", async () => {
    await ensureMediaIndexed();

    await ensureGalleryView();
    await clearSearchFilter();
    await ensureSelectionModeEnabled();

    await selectVisibleAssets(2);

    const uniqueTag = `bulk-e2e-${Date.now()}`;
    const tagInput = await $("#bulk-tag-draft-input");
    await tagInput.waitForDisplayed({ timeout: 10000 });
    await tagInput.click();
    await tagInput.setValue(uniqueTag);
    await browser.keys("Enter");
    await browser.waitUntil(async () => (await tagInput.getValue()) === "", {
      timeout: 10000,
      timeoutMsg: "bulk tag input did not clear after automatic save"
    });
    await browser.waitUntil(
      async () => {
        const page = await invokeTauriCommand("list_assets", {
          offset: 0,
          limit: 2,
          tagsAnd: [uniqueTag],
          tagsNot: [],
          kind: null,
          favoritesOnly: false,
          metaFilter: null
        });
        return page?.total === 2;
      },
      {
        timeout: 15000,
        timeoutMsg: "bulk tag write did not reach the backend"
      }
    );

    const searchInput = await $(".filter-input");
    await searchInput.waitForDisplayed({ timeout: 10000 });
    await searchInput.click();
    await searchInput.setValue(uniqueTag);
    await browser.keys("Enter");

    const noResultsHeading = await $("h3=No results");
    await noResultsHeading.waitForDisplayed({ timeout: 15000, reverse: true });

    await clearSearchFilter();

    const disableSelectionButton = await $('button[aria-label="Disable bulk actions"]');
    if (await disableSelectionButton.isExisting()) {
      await disableSelectionButton.waitForDisplayed({ timeout: 10000 });
      await disableSelectionButton.click();
    }
  });

  it("adds and clears a bulk media group from the persistent panel", async () => {
    await ensureMediaIndexed();

    await ensureGalleryView();
    await clearSearchFilter();
    await ensureSelectionModeEnabled();

    await selectVisibleAssets(2);

    const uniqueGroup = `group-e2e-${Date.now()}`;
    const groupInput = await $("#bulk-group-key-input");
    await groupInput.waitForDisplayed({ timeout: 10000 });
    if ((await groupInput.getValue()) !== "") {
      throw new Error("conflicting selections should start with an empty media group key");
    }
    if ((await $$('[data-testid="bulk-open-order-modal"]')).length !== 0) {
      throw new Error("group ordering must stay hidden until a media group key is entered");
    }
    await groupInput.click();
    await groupInput.setValue(uniqueGroup);

    await $('button=Sort in larger view').click();
    const orderTiles = await $$('[data-testid^="bulk-order-tile-"]');
    if (orderTiles.length < 2) throw new Error("sorting requires two selected assets");
    const firstAssetId = await orderTiles[0].getAttribute("data-sort-asset");
    await orderTiles[0].$('[data-sort-handle]').click();
    await browser.keys('ArrowRight');
    await browser.waitUntil(async () =>
      (await $('[data-testid^="bulk-order-tile-"]').getAttribute("data-sort-asset")) !== firstAssetId);
    await $('button=Save order').click();
    await $('[data-testid="bulk-order-modal"]').waitForExist({ reverse: true });

    const refreshedGroupInput = await $("#bulk-group-key-input");
    await refreshedGroupInput.waitForDisplayed({ timeout: 10000 });
    await browser.execute((input) => {
      const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      valueSetter?.call(input, "");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }, refreshedGroupInput);
    await browser.waitUntil(async () => (await (await $("#bulk-group-key-input")).getValue()) === "", {
      timeout: 10000,
      timeoutMsg: "bulk media group key did not clear"
    });
    const removeGroupButton = await $('[data-testid="bulk-group-panel"] > button');
    await removeGroupButton.waitForEnabled({ timeout: 10000 });
    const removeGroupLabel = await removeGroupButton.getText();
    if (removeGroupLabel !== "Remove from group" && removeGroupLabel !== "Remove from groups") {
      throw new Error(`unexpected remove-group action label: ${removeGroupLabel}`);
    }
    await removeGroupButton.click();
    await removeGroupButton.waitForEnabled({ timeout: 10000 });
  });
});

