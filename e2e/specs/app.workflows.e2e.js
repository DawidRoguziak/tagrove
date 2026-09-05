import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { copyPngFixtures, createPlayableFixtures, createLightboxFixtures } from "../fixtures.js";

const tempMediaRoots = [];
const tempArtifacts = [];

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

async function waitForSearchInputValue(expectedValue) {
  await browser.waitUntil(
    async () => {
      const searchInput = await ensureGalleryView();
      return (await searchInput.getValue()) === expectedValue;
    },
    {
      timeout: 5000,
      timeoutMsg: `Expected search input value to be "${expectedValue}"`
    }
  );
}

async function clearSearchFilter() {
  await ensureGalleryView();
  const clearFiltersButton = await $('button[aria-label="Clear all search filters"]');
  await clearFiltersButton.waitForDisplayed({ timeout: 10000 });
  await clearFiltersButton.click();
  await waitForSearchInputValue("");
}

async function submitSearchQuery(value) {
  const searchInput = await ensureGalleryView();
  await searchInput.click();
  await searchInput.setValue(value);
  await waitForSearchInputValue(value);
  await browser.keys("Escape");
  await browser.keys("Enter");
}

async function listAssetsForAssertions({
  tagsAnd = [],
  tagsNot = [],
  kind = null,
  favoritesOnly = false,
  metaFilter = null
} = {}) {
  return invokeTauriCommand("list_assets", {
    offset: 0,
    limit: 200,
    tagsAnd,
    tagsNot,
    kind,
    favoritesOnly,
    metaFilter
  });
}

async function waitForTileAtIndex(index) {
  const tile = await $(`button[data-asset-index="${index}"]`);
  await tile.waitForDisplayed({
    timeout: 15000,
    timeoutMsg: `Expected gallery tile at index ${index}`
  });
}

async function listTagsForAssertions(query = "") {
  return invokeTauriCommand("list_tags", {
    query,
    offset: 0,
    limit: 200
  });
}

async function resetLibraryState() {
  await invokeTauriCommand("clear_library_data");
  await browser.refresh();
}

function createTempFilePath(prefix, extension) {
  const artifactPath = path.join(
    os.tmpdir(),
    `media-tagger-e2e-${prefix}-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}.${extension}`
  );
  tempArtifacts.push(artifactPath);
  return artifactPath;
}

async function createTempMediaRoot(prefix, fileCount) {
  const rootDir = await fs.mkdtemp(path.join(os.tmpdir(), `media-tagger-e2e-${prefix}-`));
  tempMediaRoots.push(rootDir);

  await copyPngFixtures(rootDir, prefix, fileCount);

  return rootDir;
}

async function seedLibraryWithTempRoot(prefix, fileCount) {
  const root = await createTempMediaRoot(prefix, fileCount);
  await invokeTauriCommand("add_scan_root", { path: root });
  await invokeTauriCommand("rescan_all_roots");
  await browser.refresh();
  return root;
}

async function seedLibraryWithPlayableMedia(prefix) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), `media-tagger-e2e-${prefix}-`));
  tempMediaRoots.push(root);
  const { gifPath, videoPaths } = await createPlayableFixtures(root, prefix);

  await invokeTauriCommand("add_scan_root", { path: root });
  await invokeTauriCommand("rescan_all_roots");
  await browser.refresh();
  return { gifPath, videoPaths };
}

describe("MediaTagger desktop workflows", () => {
  afterEach(async () => {
    await invokeTauriCommand("clear_library_data");

    while (tempMediaRoots.length > 0) {
      const root = tempMediaRoots.pop();
      if (!root) {
        continue;
      }

      await fs.rm(root, { recursive: true, force: true });
    }

    while (tempArtifacts.length > 0) {
      const artifactPath = tempArtifacts.pop();
      if (!artifactPath) {
        continue;
      }

      await fs.rm(artifactPath, { force: true });
    }
  });

  it("filters by tags, media kind and favorites", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("filters", 3);

    const page = await listAssetsForAssertions();

    if (!Array.isArray(page?.items) || page.items.length < 3) {
      throw new Error("Expected at least 3 seeded assets");
    }

    const favoriteAsset = page.items[0];
    const nonFavoriteAsset = page.items[1];
    const favoriteTag = `favorite-e2e-${Date.now()}`;
    const nonFavoriteTag = `non-favorite-e2e-${Date.now()}`;

    const tagSummary = await invokeTauriCommand("set_asset_tags", {
      assetId: favoriteAsset.id,
      tags: [favoriteTag]
    });
    if (
      tagSummary?.asset_id !== favoriteAsset.id ||
      tagSummary.changed !== true ||
      !Array.isArray(tagSummary.tags) ||
      tagSummary.tags.length !== 1 ||
      tagSummary.tags[0] !== favoriteTag ||
      !Number.isSafeInteger(tagSummary.revision)
    ) {
      throw new Error(`Unexpected set_asset_tags response: ${JSON.stringify(tagSummary)}`);
    }

    await invokeTauriCommand("set_asset_tags", {
      assetId: nonFavoriteAsset.id,
      tags: [nonFavoriteTag]
    });
    await invokeTauriCommand("set_asset_favorite", {
      assetId: favoriteAsset.id,
      isFavorite: true
    });

    await browser.refresh();
    const favoritesPage = await listAssetsForAssertions({ favoritesOnly: true });
    if (favoritesPage.total !== 1) {
      throw new Error(`Expected one favorite asset, got ${favoritesPage.total}`);
    }

    const nonFavoriteTagPage = await listAssetsForAssertions({ tagsAnd: [nonFavoriteTag] });
    if (nonFavoriteTagPage.total !== 1) {
      throw new Error(`Expected one asset for non-favorite tag, got ${nonFavoriteTagPage.total}`);
    }

    const favoriteTagPage = await listAssetsForAssertions({ tagsAnd: [favoriteTag] });
    if (favoriteTagPage.total !== 1) {
      throw new Error(`Expected one asset for favorite tag, got ${favoriteTagPage.total}`);
    }

    const noResultsHeading = await $("h3=No results");

    const showFavoritesButton = await $('button[aria-label="Show favorites only"]');
    await showFavoritesButton.waitForDisplayed({ timeout: 10000 });
    await showFavoritesButton.click();

    const disableFavoritesButton = await $('button[aria-label="Disable favorites only"]');
    await disableFavoritesButton.waitForDisplayed({ timeout: 10000 });
    await disableFavoritesButton.click();

    const mediaKindSelect = await $(".filter-kind-select");
    await mediaKindSelect.waitForDisplayed({ timeout: 10000 });
    await mediaKindSelect.selectByAttribute("value", "video");
    await noResultsHeading.waitForDisplayed({ timeout: 15000 });

    await mediaKindSelect.selectByAttribute("value", "all");
    await noResultsHeading.waitForDisplayed({ timeout: 15000, reverse: true });

    const missingTagPage = await listAssetsForAssertions({ tagsAnd: [`missing-e2e-${Date.now()}`] });
    if (missingTagPage.total !== 0) {
      throw new Error(`Expected no assets for missing tag, got ${missingTagPage.total}`);
    }
  });

  it("filters by exact tag counts with tags and tags:N", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("tag-count", 4);

    const page = await listAssetsForAssertions();
    if (!Array.isArray(page?.items) || page.items.length < 4) {
      throw new Error("Expected at least 4 seeded assets for tag count filters");
    }

    await invokeTauriCommand("set_asset_tags", {
      assetId: page.items[1].id,
      tags: ["count-a", "count-b"]
    });
    await invokeTauriCommand("set_asset_tags", {
      assetId: page.items[2].id,
      tags: ["count-c", "count-d"]
    });
    await invokeTauriCommand("set_asset_tags", {
      assetId: page.items[3].id,
      tags: ["single-tag"]
    });

    await browser.refresh();

    const zeroTagPage = await listAssetsForAssertions({
      metaFilter: { type: "hasNoTags", tagCount: 0 }
    });
    if (zeroTagPage.total !== 1) {
      throw new Error(`Expected one zero-tag asset, got ${zeroTagPage.total}`);
    }

    await submitSearchQuery("tags");
    await waitForTileAtIndex(0);

    const noResultsHeading = await $("h3=No results");
    await noResultsHeading.waitForDisplayed({ timeout: 5000, reverse: true });

    await clearSearchFilter();
    await waitForTileAtIndex(3);

    const exactTwoTagsPage = await listAssetsForAssertions({
      metaFilter: { type: "hasNoTags", tagCount: 2 }
    });
    if (exactTwoTagsPage.total !== 2) {
      throw new Error(`Expected two exact-two-tag assets, got ${exactTwoTagsPage.total}`);
    }

    await submitSearchQuery("tags:2");
    await waitForTileAtIndex(1);
    await noResultsHeading.waitForDisplayed({ timeout: 5000, reverse: true });

    await clearSearchFilter();
    await waitForTileAtIndex(3);
  });

  it("filters by exact group name case-insensitively with gN", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("group-name", 4);

    const page = await listAssetsForAssertions();
    if (!Array.isArray(page?.items) || page.items.length < 4) {
      throw new Error("Expected at least 4 seeded assets for group filters");
    }

    await invokeTauriCommand("set_asset_media_group", {
      assetId: page.items[0].id,
      mediaGroupKey: "Trip-2026",
      mediaGroupOrder: 1
    });
    await invokeTauriCommand("set_asset_media_group", {
      assetId: page.items[1].id,
      mediaGroupKey: "trip-2026",
      mediaGroupOrder: 2
    });
    await invokeTauriCommand("set_asset_media_group", {
      assetId: page.items[2].id,
      mediaGroupKey: "Trip-2027",
      mediaGroupOrder: 1
    });

    await browser.refresh();

    const matchingGroupPage = await listAssetsForAssertions({
      metaFilter: { type: "groupName", groupName: "trip-2026" }
    });
    if (matchingGroupPage.total !== 2) {
      throw new Error(`Expected two exact group-name matches, got ${matchingGroupPage.total}`);
    }

    await submitSearchQuery("gN:trip-2026");
    await waitForTileAtIndex(1);

    const noResultsHeading = await $("h3=No results");
    await noResultsHeading.waitForDisplayed({ timeout: 5000, reverse: true });

    await clearSearchFilter();
    await waitForTileAtIndex(3);

    await submitSearchQuery("gN:TRIP-2026");
    await waitForTileAtIndex(1);
    await noResultsHeading.waitForDisplayed({ timeout: 5000, reverse: true });

    await clearSearchFilter();
    await waitForTileAtIndex(3);
  });

  it("removes scan root from settings and returns to empty library", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("remove-root", 2);

    await ensureSettingsView();

    const removeButton = await $("button=Remove");
    await removeButton.waitForDisplayed({ timeout: 10000 });
    await removeButton.click();

    const confirmTitle = await $("h3=Remove scan path?");
    await confirmTitle.waitForDisplayed({ timeout: 10000 });

    const confirmRemoveButton = await $("//div[@role='dialog']//button[normalize-space()='Remove']");
    await confirmRemoveButton.waitForDisplayed({ timeout: 10000 });
    await confirmRemoveButton.click();

    await confirmTitle.waitForDisplayed({ timeout: 15000, reverse: true });

    const backButton = await $('button[aria-label="Back"]');
    await backButton.waitForDisplayed({ timeout: 10000 });
    await backButton.click();

    const noFoldersHeading = await $("h3=No folders attached");
    await noFoldersHeading.waitForDisplayed({ timeout: 15000 });
  });

  it("clears library data from danger zone after YES confirmation", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("danger-clear", 2);

    await ensureSettingsView();

    const dangerButton = await $("button=Remove all thumbnails, indexed assets and scan paths");
    await dangerButton.waitForDisplayed({ timeout: 10000 });
    await dangerButton.click();

    const confirmDialogTitle = await $("h3=Delete all library data?");
    await confirmDialogTitle.waitForDisplayed({ timeout: 10000 });

    const confirmYesButton = await $("button=YES");
    await confirmYesButton.waitForDisplayed({ timeout: 10000 });
    await confirmYesButton.click();

    await confirmDialogTitle.waitForDisplayed({ timeout: 15000, reverse: true });

    const backButton = await $('button[aria-label="Back"]');
    await backButton.waitForDisplayed({ timeout: 10000 });
    await backButton.click();

    const noFoldersHeading = await $("h3=No folders attached");
    await noFoldersHeading.waitForDisplayed({ timeout: 15000 });
  });

  it("keeps lightbox panels inside the viewport and remembers collapse while browsing", async () => {
    await resetLibraryState();
    const root = await createTempMediaRoot("lightbox-layout", 0);
    await createLightboxFixtures(root);
    await invokeTauriCommand("add_scan_root", { path: root });
    await invokeTauriCommand("rescan_all_roots");
    const fixturePage = await listAssetsForAssertions();
    for (const asset of fixturePage.items) {
      await invokeTauriCommand("set_asset_tags", { assetId: asset.id, tags: ["long_tag_".repeat(30)] });
    }
    await browser.refresh();
    await ensureGalleryView();
    const original = await browser.getWindowSize();
    const evidence = path.resolve("artifacts/lightbox-layout");
    await fs.mkdir(evidence, { recursive: true });
    try {
      for (const [width, height] of [[1440, 900], [1000, 720], [600, 400], [320, 360]]) {
        for (let index = 0; index < 3; index++) {
          // Gallery layout is outside this test: open at the normal desktop size,
          // then exercise the lightbox's own responsive transition.
          await browser.setWindowSize(1440, 900);
          await waitForTileAtIndex(index);
          await $(`button[data-asset-index="${index}"]`).scrollIntoView({ block: "center" });
          await $(`button[data-asset-index="${index}"]`).click();
          await browser.setWindowSize(width, height);
          if (width < 768) {
            await $('button[aria-label="Open asset panel"]').waitForDisplayed();
            await $('button[aria-label="Open asset panel"]').click();
          }
          await browser.waitUntil(() => browser.execute(() => {
            const img = document.querySelector('[data-testid="lightbox-image"]');
            return img?.complete && img.naturalWidth > 0;
          }));
          await browser.waitUntil(() => browser.execute(() => {
            const sidebar = document.querySelector("[data-lightbox-toolbar]");
            const stage = document.querySelector("[data-lightbox-media-stage]");
            const dialog = document.querySelector("[data-lightbox-kind]");
            const upper = document.querySelector('[data-testid="lightbox-sidebar-upper"]');
            const rail = document.querySelector('[data-testid="lightbox-action-rail"]');
            const image = document.querySelector('[data-testid="lightbox-image"]');
            const panelRect = sidebar.getBoundingClientRect();
            const stageRect = stage.getBoundingClientRect();
            const dialogRect = dialog.getBoundingClientRect();
            const imageRect = image.getBoundingClientRect();
            return (innerWidth < 768 || panelRect.left >= stageRect.right - 1) && panelRect.right <= dialogRect.right + 1
              && dialogRect.bottom <= innerHeight && dialogRect.top >= 0
              && sidebar.scrollWidth <= sidebar.clientWidth + 1 && upper.scrollWidth <= upper.clientWidth + 1
              && rail.getBoundingClientRect().bottom <= panelRect.bottom
              && imageRect.width <= stageRect.width + 1 && imageRect.height <= stageRect.height + 1;
          }), { timeout: 5000, timeoutMsg: "Lightbox content overflows its assigned viewport" });
          await browser.saveScreenshot(path.join(evidence, `${width}-${index}-open.png`));
          await $('button[aria-label="Close asset panel"]').click();
          await browser.waitUntil(async () => (await $("[data-lightbox-toolbar]").getAttribute("aria-hidden")) === "true");
          await browser.waitUntil(() => browser.execute(() =>
            getComputedStyle(document.querySelector("#lightbox-sidebar-trigger-button")).opacity === "0"
          ), { timeout: 6000, timeoutMsg: "Collapsed panel trigger did not fade after inactivity" });
          await browser.saveScreenshot(path.join(evidence, `${width}-${index}-idle.png`));
          await browser.keys("ArrowRight");
          await $('button[aria-label="Open asset panel"]').waitForDisplayed();
          await browser.waitUntil(async () => (await $("[data-lightbox-toolbar]").getAttribute("aria-hidden")) === "true");
          await $('button[aria-label="Open asset panel"]').click();
          await $('button[aria-label="Close asset panel"]').waitForDisplayed();
          await browser.keys("Escape");
          if (width < 768) await browser.keys("Escape");
        }
      }
    } catch (error) {
      console.log("LIGHTBOX_BOUNDS", JSON.stringify(await browser.execute(() =>
        ["[data-lightbox-kind]", "[data-lightbox-toolbar]", "[data-lightbox-media-stage]", '[data-testid="lightbox-sidebar-upper"]', '[data-testid="lightbox-action-rail"]', '[data-testid="lightbox-image"]'].map((selector) => {
          const el = document.querySelector(selector);
          return { selector, rect: el?.getBoundingClientRect().toJSON(), clientWidth: el?.clientWidth, scrollWidth: el?.scrollWidth, viewport: [innerWidth, innerHeight] };
        })
      )));
      await browser.saveScreenshot(path.join(evidence, "failure.png"));
      throw error;
    } finally {
      if (await $("[data-lightbox-kind]").isExisting()) await browser.keys("Escape");
      await browser.setWindowSize(original.width, original.height);
    }
  });

  it("keeps the windowed video lightbox toggle outside native bounds", async () => {
    await resetLibraryState();
    await seedLibraryWithPlayableMedia("lightbox-video");
    const page = await listAssetsForAssertions({ kind: "video" });
    await ensureGalleryView();
    await $(`button[data-asset-id="${page.items[0].id}"]`).click();
    await $('button[aria-label="Close asset panel"]').waitForDisplayed();
    await $('button[aria-label="Close asset panel"]').click();
    await browser.waitUntil(() => browser.execute(() => {
      const video = document.querySelector("[data-lightbox-video-player]").getBoundingClientRect();
      const trigger = document.querySelector("#lightbox-sidebar-trigger-button").getBoundingClientRect();
      return video.right <= trigger.left + 1;
    }));
    const beforeIdle = await $("[data-lightbox-video-player]").getSize();
    await browser.waitUntil(() => browser.execute(() =>
      getComputedStyle(document.querySelector("#lightbox-sidebar-trigger-button")).opacity === "0"
    ), { timeout: 6000 });
    const afterIdle = await $("[data-lightbox-video-player]").getSize();
    if (JSON.stringify(beforeIdle) !== JSON.stringify(afterIdle)) throw new Error("Idle video bounds changed");
    await browser.keys("Tab");
    await $('button[aria-label="Open asset panel"]').click();
    await $('button[aria-label="Close asset panel"]').waitForDisplayed();
    await browser.waitUntil(() => browser.execute(() => {
      const video = document.querySelector("[data-lightbox-video-player]").getBoundingClientRect();
      const sidebar = document.querySelector("[data-lightbox-toolbar]").getBoundingClientRect();
      return video.right <= sidebar.left + 1;
    }));
    await browser.keys("Escape");
  });

  it("uses a drawer for lightbox actions in a narrow window", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("lightbox-drawer", 1);
    await ensureGalleryView();

    const originalWindowSize = await browser.getWindowSize();
    try {
      await browser.setWindowSize(600, 760);
      const firstTile = await $('button[data-asset-index="0"]');
      await firstTile.waitForDisplayed({ timeout: 15000 });
      await firstTile.click();

      const openPanelButton = await $('button[aria-label="Open asset panel"]');
      await openPanelButton.waitForDisplayed({ timeout: 10000 });
      const sidebar = await $("[data-lightbox-toolbar]");
      await browser.waitUntil(async () => (await sidebar.getAttribute("aria-hidden")) === "true", {
        timeout: 5000,
        timeoutMsg: "Expected the narrow lightbox sidebar to start closed"
      });

      await browser.keys("Tab");
      await openPanelButton.click();
      const tagInput = await $("#lightbox-tag-draft-input");
      await tagInput.waitForDisplayed({ timeout: 10000 });
      await browser.waitUntil(async () => (await sidebar.getAttribute("aria-hidden")) === "false", {
        timeout: 5000,
        timeoutMsg: "Expected the lightbox drawer to open"
      });

      const editorOrderIsCorrect = await browser.execute(() => {
        const group = document.querySelector('[data-testid="lightbox-media-group-panel"]');
        const tags = document.querySelector('[data-testid="lightbox-tag-panel"]');
        return Boolean(group && tags && (group.compareDocumentPosition(tags) & Node.DOCUMENT_POSITION_FOLLOWING));
      });
      if (!editorOrderIsCorrect) {
        throw new Error("Expected media group controls above tagging in the lightbox drawer");
      }

      await browser.keys("Escape");
      const reopenedPanelButton = await $('button[aria-label="Open asset panel"]');
      await reopenedPanelButton.waitForDisplayed({ timeout: 10000 });
      const dialog = await $('[role="dialog"][data-lightbox-kind]');
      await dialog.waitForDisplayed({ timeout: 5000 });
      await browser.keys("Escape");
      await dialog.waitForDisplayed({ timeout: 10000, reverse: true });
    } finally {
      const dialog = await $('[role="dialog"][data-lightbox-kind]');
      if (await dialog.isExisting()) {
        await browser.keys("Escape");
        if (await dialog.isExisting()) await browser.keys("Escape");
      }
      await browser.setWindowSize(originalWindowSize.width, originalWindowSize.height);
    }
  });

  it("supports lightbox tagging, media group editing and delete confirmation", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("lightbox", 2);

    await ensureGalleryView();

    const beforeDeletePage = await listAssetsForAssertions();
    const beforeDeleteTotal = beforeDeletePage.total;
    if (beforeDeleteTotal < 2) {
      throw new Error("Expected at least 2 seeded assets for lightbox workflow");
    }

    const firstTile = await $('button[data-asset-index="0"]');
    await firstTile.waitForDisplayed({ timeout: 15000 });
    await firstTile.click();

    const closePreviewButton = await $('button[aria-label="Close preview"]');
    await closePreviewButton.waitForDisplayed({ timeout: 10000 });

    const addToFavoritesButton = await $('button[aria-label="Add to favorites"]');
    await addToFavoritesButton.waitForDisplayed({ timeout: 10000 });
    await addToFavoritesButton.click();

    const removeFromFavoritesButton = await $('button[aria-label="Remove from favorites"]');
    await removeFromFavoritesButton.waitForDisplayed({ timeout: 10000 });

    const uniqueTag = `lightbox-e2e-${Date.now()}`;
    const tagInput = await $("#lightbox-tag-draft-input");
    await tagInput.waitForDisplayed({ timeout: 10000 });
    await tagInput.click();
    await tagInput.setValue(uniqueTag);
    await browser.keys("Enter");

    const groupKeyInput = await $("#lightbox-media-group-key-input");
    await groupKeyInput.waitForDisplayed({ timeout: 10000 });
    await groupKeyInput.setValue(`group-${Date.now()}`);

    const groupOrderInput = await $("#lightbox-media-group-order-input");
    await groupOrderInput.waitForDisplayed({ timeout: 10000 });
    await groupOrderInput.setValue("2");

    const applyGroupButton = await $("button=Apply");
    await applyGroupButton.waitForDisplayed({ timeout: 10000 });
    await applyGroupButton.click();

    await closePreviewButton.click();
    await closePreviewButton.waitForDisplayed({ timeout: 10000, reverse: true });

    await browser.waitUntil(
      async () => {
        const taggedPage = await listAssetsForAssertions({ tagsAnd: [uniqueTag] });
        return taggedPage.total === 1;
      },
      {
        timeout: 15000,
        timeoutMsg: "Expected exactly one asset tagged in lightbox"
      }
    );

    await clearSearchFilter();

    const tileToDelete = await $('button[data-asset-index="0"]');
    await tileToDelete.waitForDisplayed({ timeout: 10000 });
    await tileToDelete.click();

    const deleteMediaButton = await $('button[aria-label="Delete media"]');
    await deleteMediaButton.waitForDisplayed({ timeout: 10000 });
    await deleteMediaButton.click();

    const deleteDialogTitle = await $("h3=Are you sure you want to delete this media?");
    await deleteDialogTitle.waitForDisplayed({ timeout: 10000 });

    const confirmInput = await $("#lightbox-delete-confirm-input");
    await confirmInput.waitForDisplayed({ timeout: 10000 });
    await confirmInput.setValue("Yes");

    const confirmDeleteButton = await $("//div[@role='dialog']//button[normalize-space()='Confirm']");
    await confirmDeleteButton.waitForEnabled({ timeout: 10000 });
    await confirmDeleteButton.click();

    await deleteDialogTitle.waitForDisplayed({ timeout: 15000, reverse: true });
    await closePreviewButton.waitForDisplayed({ timeout: 10000, reverse: true });

    await browser.waitUntil(
      async () => {
        const afterDeletePage = await listAssetsForAssertions();
        return afterDeletePage.total === beforeDeleteTotal - 1;
      },
      {
        timeout: 15000,
        timeoutMsg: "Expected one asset removed after delete confirmation"
      }
    );
  });

  it("keeps native playback state through paused seeking, fullscreen and replacement", async () => {
    await resetLibraryState();
    await seedLibraryWithPlayableMedia("native-state");
    await ensureGalleryView();
    await $(".filter-kind-select").selectByAttribute("value", "video");
    await waitForTileAtIndex(1);
    await $('button[data-asset-index="0"]').click();
    const selector = "[data-native-video-active]";
    const player = await $(selector);
    await player.waitForExist({ timeout: 15000 });
    const sessionId = Number(await player.getAttribute("data-native-session"));
    const control = (command) => invokeTauriCommand("control_video", { sessionId, command });
    const waitAttribute = (name, value) => browser.waitUntil(async () =>
      (await $(selector).getAttribute(`data-native-${name}`)) === value,
      { timeout: 10000, timeoutMsg: `Expected native ${name}=${value}` });
    await browser.waitUntil(async () => Number(await player.getAttribute("data-native-time")) > 0,
      { timeout: 15000, timeoutMsg: "Expected native playback to advance" });
    await control({ type: "pause" });
    await control({ type: "setMuted", muted: true });
    await control({ type: "setVolume", volume: 0.4 });
    await control({ type: "setRate", rate: 1.5 });
    await waitAttribute("paused", "true");
    await waitAttribute("muted", "true");
    await waitAttribute("volume", "0.4");
    await waitAttribute("rate", "1.5");
    await control({ type: "seek", time: 3 });
    await browser.waitUntil(async () => Number(await player.getAttribute("data-native-time")) >= 2.9,
      { timeout: 10000 });
    await waitAttribute("seeking", "false");
    await waitAttribute("paused", "true");
    await player.click();
    await browser.keys("f");
    await browser.waitUntil(async () => (await player.getAttribute("data-native-fullscreen")) !== null,
      { timeout: 10000 });
    if (Number(await player.getAttribute("data-native-session")) !== sessionId) throw new Error("Fullscreen replaced the session");
    await waitAttribute("paused", "true");
    await browser.keys("Escape");
    await browser.waitUntil(async () => (await player.getAttribute("data-native-fullscreen")) === null,
      { timeout: 10000 });
    if (Number(await player.getAttribute("data-native-session")) !== sessionId) throw new Error("Exiting fullscreen replaced the session");
    await player.click();
    await browser.keys("f");
    await browser.waitUntil(async () => (await player.getAttribute("data-native-fullscreen")) !== null, { timeout: 10000 });
    await browser.execute(() => document.querySelector('[role="dialog"]').focus());
    await browser.keys("ArrowRight");
    await browser.waitUntil(async () => Number(await $(selector).getAttribute("data-native-session")) !== sessionId,
      { timeout: 10000 });
    await waitAttribute("paused", "false");
    await browser.waitUntil(async () => (await $(selector).getAttribute("data-native-fullscreen")) === null, { timeout: 10000 });
    await $('button[aria-label="Close preview"]').waitForDisplayed();
    await waitAttribute("muted", "true");
    await waitAttribute("volume", "0.4");
    await waitAttribute("rate", "1.5");
    // A delayed close from the retired activation must not stop its replacement.
    await invokeTauriCommand("close_video", { sessionId });
    await waitAttribute("paused", "false");
    await browser.keys("Escape");
    await $(selector).waitForExist({ reverse: true });
  });

  it("loads generated GIF and continuously switches and loops longer MP4 sources", async () => {
    await resetLibraryState();
    const { gifPath, videoPaths } = await seedLibraryWithPlayableMedia("playback");
    const assetsPage = await listAssetsForAssertions();
    const gifAsset = assetsPage.items.find((asset) => asset.path === gifPath);
    const videoAssets = videoPaths.map((videoPath) =>
      assetsPage.items.find((asset) => asset.path === videoPath)
    );
    if (!gifAsset || videoAssets.some((asset) => !asset)) {
      throw new Error("Expected generated GIF and two MP4 assets after scan");
    }

    const query = await invokeTauriCommand("start_asset_query", {
      tagsAnd: [],
      tagsNot: [],
      kind: null,
      favoritesOnly: false,
      metaFilter: null,
      generation: 1,
      pageSize: 20
    });
    const gifSummary = query.items.find((asset) => asset.id === gifAsset.id);
    const videoSummaries = videoAssets.map((videoAsset) =>
      query.items.find((asset) => asset.id === videoAsset.id)
    );
    if (
      gifSummary?.preview_path !== gifPath ||
      videoSummaries.some((summary, index) => summary?.preview_path !== videoPaths[index])
    ) {
      throw new Error("Expected GIF and both video summaries to contain full preview paths");
    }

    await ensureGalleryView();
    const gifTile = await $(`button[data-asset-id="${gifAsset.id}"]`);
    await gifTile.waitForDisplayed({ timeout: 15000 });
    await gifTile.click();

    const lightboxImage = await $('[data-testid="lightbox-image"]');
    await lightboxImage.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(async () => Number(await lightboxImage.getProperty("naturalWidth")) > 0, {
      timeout: 10000,
      timeoutMsg: "Expected generated GIF to decode in the lightbox"
    });
    if (await $('[data-testid="lightbox-media-error"]').isExisting()) {
      throw new Error("Generated GIF displayed the media error fallback");
    }

    const closePreviewButton = await $('button[aria-label="Close preview"]');
    await closePreviewButton.click();
    await closePreviewButton.waitForDisplayed({ timeout: 10000, reverse: true });

    const mediaKindSelect = await $(".filter-kind-select");
    await mediaKindSelect.selectByAttribute("value", "video");
    await waitForTileAtIndex(1);
    const videoQuery = await invokeTauriCommand("start_asset_query", {
      tagsAnd: [],
      tagsNot: [],
      kind: "video",
      favoritesOnly: false,
      metaFilter: null,
      generation: 2,
      pageSize: 20
    });
    if (!Array.isArray(videoQuery.items) || videoQuery.items.length !== 2) {
      throw new Error("Expected exactly two generated videos in the filtered query");
    }
    const [firstVideo, secondVideo] = videoQuery.items;

    const firstVideoTile = await $(`button[data-asset-id="${firstVideo.id}"]`);
    await firstVideoTile.waitForDisplayed({ timeout: 15000 });
    await firstVideoTile.click();

    const player = await $("[data-lightbox-video-player]");
    await player.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(async () => (await player.getAttribute("aria-label")) === firstVideo.preview_path, {
      timeout: 10000,
      timeoutMsg: "Expected the first generated MP4 in the lightbox"
    });
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            document.querySelectorAll("video").length === 0 &&
            document.querySelectorAll(".native-video-mask").length === 0 &&
            getComputedStyle(document.querySelector("#root")).visibility === "visible"
        ),
      {
        timeout: 15000,
        timeoutMsg: "Expected an active native video surface without an HTML video element"
      }
    );
    const videoLightboxGeometry = JSON.parse(
      await browser.execute(() => {
        const dialog = document.querySelector('[role="dialog"][data-lightbox-kind="video"]');
        const currentPlayer = document.querySelector("[data-lightbox-video-player]");
        const controls = currentPlayer?.querySelector(".media-controls");
        if (
          !(dialog instanceof HTMLElement) ||
          !(currentPlayer instanceof HTMLElement)
        ) {
          throw new Error("Expected video lightbox geometry targets");
        }
        const dialogRect = dialog.getBoundingClientRect();
        const playerRect = currentPlayer.getBoundingClientRect();
        const playerStyle = getComputedStyle(currentPlayer);

        const aspectParts = playerStyle.aspectRatio.split("/").map(Number);
        const declaredAspect = aspectParts[0] / aspectParts[1];
        const renderedAspect = playerRect.width / playerRect.height;
        return JSON.stringify({
          gaps: {
            top: dialogRect.top,
            right: window.innerWidth - dialogRect.right,
            bottom: window.innerHeight - dialogRect.bottom,
            left: dialogRect.left
          },
          player: {
            borderTopWidth: playerStyle.borderTopWidth,
            borderRadius: playerStyle.borderRadius,
            boxShadow: playerStyle.boxShadow,
            aspectDelta: Math.abs(declaredAspect - renderedAspect)
          },
          domControlsPresent: controls !== null
        });
      })
    );
    for (const [edge, gap] of Object.entries(videoLightboxGeometry.gaps)) {
      if (gap < 19.5) {
        throw new Error(
          `Expected at least 20px video lightbox clearance at ${edge}, received ${gap}`
        );
      }
    }
    if (
      videoLightboxGeometry.player.borderTopWidth !== "0px" ||
      videoLightboxGeometry.player.borderRadius !== "0px" ||
      videoLightboxGeometry.player.boxShadow !== "none"
    ) {
      throw new Error(
        `Expected a borderless video player inside the lightbox frame: ${JSON.stringify(
          videoLightboxGeometry.player
        )}`
      );
    }
    if (
      videoLightboxGeometry.player.aspectDelta > 0.02 ||
      videoLightboxGeometry.domControlsPresent
    ) {
      throw new Error(
        `Expected the full native player footprint without a separate DOM control rail: ${JSON.stringify(
          videoLightboxGeometry
        )}`
      );
    }
    await browser.execute(() => {
      for (const key of ["ArrowRight", "ArrowLeft", "ArrowRight"]) {
        window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
      }
    });
    await browser.waitUntil(
      async () => {
        const currentPlayer = await $("[data-lightbox-video-player]");
        return (
          (await currentPlayer.getAttribute("aria-label")) === secondVideo.preview_path &&
          (await browser.execute(() => document.querySelectorAll("video").length)) === 0
        );
      },
      {
        timeout: 10000,
        timeoutMsg: "Expected rapid navigation to settle on the latest video target"
      }
    );
    await browser.execute(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
    });
    await browser.waitUntil(
      async () => {
        const currentPlayer = await $("[data-lightbox-video-player]");
        return (
          (await currentPlayer.getAttribute("aria-label")) === firstVideo.preview_path &&
          (await browser.execute(() => document.querySelectorAll("video").length)) === 0
        );
      },
      {
        timeout: 10000,
        timeoutMsg: "Expected navigation back to the first generated video"
      }
    );

    if (await $('[data-testid="lightbox-media-error"]').isExisting()) {
      throw new Error("Generated MP4 displayed the media error fallback");
    }
    const closeVideoPreviewButton = await $('button[aria-label="Close preview"]');
    await closeVideoPreviewButton.click();
    await closeVideoPreviewButton.waitForDisplayed({ timeout: 10000, reverse: true });
  });

  it("roundtrips tags through CSV export/import commands", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("csv-roundtrip", 2);

    const page = await listAssetsForAssertions();
    if (!Array.isArray(page?.items) || page.items.length < 1) {
      throw new Error("Expected at least one seeded asset for CSV roundtrip");
    }

    const targetAsset = page.items[0];
    const csvTag = `csv-roundtrip-${Date.now()}`;

    await invokeTauriCommand("set_asset_tags", {
      assetId: targetAsset.id,
      tags: [csvTag]
    });

    const csvPath = createTempFilePath("tags-roundtrip", "csv");
    const exportSummary = await invokeTauriCommand("export_tags_csv", { path: csvPath });
    if (!exportSummary || typeof exportSummary.rows !== "number" || exportSummary.rows < 1) {
      throw new Error("Expected CSV export to produce at least one row");
    }

    await invokeTauriCommand("set_asset_tags", {
      assetId: targetAsset.id,
      tags: []
    });

    const beforeImport = await listAssetsForAssertions({ tagsAnd: [csvTag] });
    if (beforeImport.total !== 0) {
      throw new Error("Expected tag to be absent before CSV import");
    }

    const importSummary = await invokeTauriCommand("import_tags_csv", { path: csvPath });
    if (!importSummary || importSummary.assets_updated < 1) {
      throw new Error("Expected CSV import to update at least one asset");
    }

    const afterImport = await listAssetsForAssertions({ tagsAnd: [csvTag] });
    if (afterImport.total !== 1) {
      throw new Error(`Expected exactly one asset restored by CSV import, got ${afterImport.total}`);
    }
  });

  it("restores library state from DB bundle export/import commands", async () => {
    await resetLibraryState();
    await seedLibraryWithTempRoot("db-roundtrip", 2);

    const beforeExport = await listAssetsForAssertions();
    if (!Array.isArray(beforeExport?.items) || beforeExport.items.length < 1) {
      throw new Error("Expected seeded assets before DB bundle export");
    }

    const targetAsset = beforeExport.items[0];
    const backupTag = `db-roundtrip-${Date.now()}`;

    await invokeTauriCommand("set_asset_tags", {
      assetId: targetAsset.id,
      tags: [backupTag]
    });

    const bundlePath = createTempFilePath("db-bundle-roundtrip", "zip");
    const exportSummary = await invokeTauriCommand("export_db_bundle", { path: bundlePath });
    if (!exportSummary || exportSummary.copied_files < 1) {
      throw new Error("Expected DB bundle export to include at least one file");
    }

    await invokeTauriCommand("clear_library_data");

    const cleared = await listAssetsForAssertions();
    if (cleared.total !== 0) {
      throw new Error(`Expected empty library before DB restore, got ${cleared.total} assets`);
    }

    const inspection = await invokeTauriCommand("inspect_db_bundle", { path: bundlePath });
    if (inspection?.format_version !== 2 || inspection?.requires_mapping !== false) {
      throw new Error("Expected a versioned same-platform DB bundle inspection");
    }
    const importSummary = await invokeTauriCommand("import_db_bundle", {
      path: bundlePath,
      rootMappings: []
    });
    if (!importSummary || importSummary.restored_files < 1) {
      throw new Error("Expected DB bundle import to restore at least one file");
    }

    const restoredByTag = await listAssetsForAssertions({ tagsAnd: [backupTag] });
    if (restoredByTag.total !== 1) {
      throw new Error(`Expected restored tag match after DB import, got ${restoredByTag.total}`);
    }

    const allTagsPage = await listTagsForAssertions("");
    if (!Array.isArray(allTagsPage?.items) || !allTagsPage.items.includes(backupTag)) {
      throw new Error("Expected restored tags list to contain backup tag after DB import");
    }
  });
});






