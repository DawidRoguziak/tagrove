import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const iconsRoot = path.resolve(__dirname, "..", "..", "src-tauri", "icons");
const execFileAsync = promisify(execFile);

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

  const sourceEntries = await fs.readdir(iconsRoot);
  const pngSources = sourceEntries
    .filter((entry) => entry.toLowerCase().endsWith(".png"))
    .map((entry) => path.join(iconsRoot, entry));

  if (!pngSources.length) {
    throw new Error("Cannot seed temp media root: no PNG source files found.");
  }

  for (let index = 0; index < fileCount; index += 1) {
    const sourcePath = pngSources[index % pngSources.length];
    const targetPath = path.join(rootDir, `${prefix}-${index + 1}.png`);
    await fs.copyFile(sourcePath, targetPath);
  }

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
  const gifPath = path.join(root, `${prefix}.gif`);
  const videoPath = path.join(root, `${prefix}.mp4`);
  const ffmpeg = process.env.FFMPEG_PATH ?? "ffmpeg";

  await execFileAsync(ffmpeg, [
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc=size=96x64:rate=8",
    "-t",
    "1",
    "-y",
    gifPath
  ]);
  await execFileAsync(ffmpeg, [
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=160x90:rate=24",
    "-f",
    "lavfi",
    "-i",
    "sine=frequency=440:sample_rate=48000",
    "-t",
    "3",
    "-c:v",
    "mpeg4",
    "-q:v",
    "5",
    "-c:a",
    "aac",
    "-shortest",
    "-y",
    videoPath
  ]);

  await invokeTauriCommand("add_scan_root", { path: root });
  await invokeTauriCommand("rescan_all_roots");
  await browser.refresh();
  return { gifPath, videoPath };
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

    const taggingButton = await $('button[aria-label="Show tagging"]');
    await taggingButton.waitForDisplayed({ timeout: 10000 });
    await taggingButton.click();

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

  it("loads generated GIF and MP4 sources in the lightbox", async () => {
    await resetLibraryState();
    const { gifPath, videoPath } = await seedLibraryWithPlayableMedia("playback");
    const assetsPage = await listAssetsForAssertions();
    const gifAsset = assetsPage.items.find((asset) => asset.path === gifPath);
    const videoAsset = assetsPage.items.find((asset) => asset.path === videoPath);
    if (!gifAsset || !videoAsset) {
      throw new Error("Expected generated GIF and MP4 assets after scan");
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
    const videoSummary = query.items.find((asset) => asset.id === videoAsset.id);
    if (gifSummary?.preview_path !== gifPath || videoSummary?.preview_path !== videoPath) {
      throw new Error("Expected GIF and video summaries to contain full preview paths");
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

    const videoTile = await $(`button[data-asset-id="${videoAsset.id}"]`);
    await videoTile.waitForDisplayed({ timeout: 15000 });
    await videoTile.click();

    const video = await $("[data-lightbox-video-player] video");
    await video.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(async () => Number(await video.getProperty("readyState")) >= 2, {
      timeout: 15000,
      timeoutMsg: "Expected generated MP4 to reach playable readyState"
    });
    const initialTime = Number(await video.getProperty("currentTime"));
    await browser.waitUntil(
      async () => Number(await video.getProperty("currentTime")) > initialTime + 0.2,
      {
        timeout: 10000,
        timeoutMsg: "Expected generated MP4 playback to advance"
      }
    );
    if (await $('[data-testid="lightbox-media-error"]').isExisting()) {
      throw new Error("Generated MP4 displayed the media error fallback");
    }
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
    if (inspection?.format_version !== 1 || inspection?.requires_mapping !== false) {
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






