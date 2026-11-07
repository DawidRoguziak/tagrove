import { createLightbox } from "../features/lightbox/createLightbox";
import { createBulk } from "../features/bulk/createBulk";
import { getStartupPopularTags } from "../platform/tauri/tags";
import { createSearch } from "../features/search/createSearch";
import { createSelection } from "../features/gallery/createSelection";
import { SuggestionClient } from "../features/search/suggestions/SuggestionClient";
import { createSettings } from "../features/settings/createSettings";
import {
  clampTileSize,
  DEFAULT_TILE_SIZE,
  getNextTileSizeByWheel
} from "../features/gallery/tileSizeService";
import { createLibrary } from "../library/createLibrary";
import { createSignal, onCleanup } from "solid-js";
import { applyTheme, readInitialTheme, type AppTheme } from "../ui/dom/theme";

export function createAppRuntime() {
  const [view, setView] = createSignal<"gallery" | "settings">("gallery");
  const [theme, setTheme] = createSignal(readInitialTheme());
  const library = createLibrary();
  const lightbox = createLightbox(library);
  const search = createSearch(library.applyFilters);
  const selection = createSelection({
    queryEpoch: () => library.query.queryEpoch,
    identityEpoch: () => library.tags.epoch,
    getIdsRange: library.query.getIdsRangeAsync
  });
  const bulk = createBulk(library, selection, () => view() === "settings");
  const [popularTags, setPopularTags] = createSignal<string[]>([]);
  const suggestions = new SuggestionClient();
  let disposed = false;
  let started = false;
  let size = DEFAULT_TILE_SIZE;
  const [tileSize, publishTileSize] = createSignal(size);
  const [maintenanceLoading, setMaintenanceLoading] = createSignal(false);
  const [highlightScan, setHighlightScan] = createSignal(false);
  const settings = createSettings({
    setLoading: setMaintenanceLoading,
    refreshLibrary: library.refreshLibrary,
    onImportDbRestored: library.handleImportDbRestored,
    onLibraryCleared: library.handleLibraryCleared,
    onTagCacheInvalidated: library.resetIdentity,
    runWithTagMutationBarrier: library.tags.runWithMutationBarrier
  });
  const setTileSize = (next: number) => {
    size = clampTileSize(next);
    publishTileSize(size);
  };
  onCleanup(() => {
    disposed = true;
    suggestions.dispose();
  });
  return {
    lightbox,
    bulk,
    popularTags,
    search,
    selection,
    suggestions,
    tileSize,
    setTileSize,
    settings,
    maintenanceLoading,
    highlightScan,
    hasScanRoots: () => settings.scan.scanRoots.length > 0,
    openFirstFolderSettings() {
      setHighlightScan(true);
      setView("settings");
    },
    zoom: (delta: number) => setTileSize(getNextTileSizeByWheel(size, delta)),
    start() {
      if (started) return;
      started = true;
      void getStartupPopularTags()
        .then((tags) => {
          if (!disposed) setPopularTags(tags);
        })
        .catch((error) => {
          if (!disposed) console.error("Failed to load startup popular tags", error);
        });
      void library.refreshLibrary().catch(() => {});
      void settings.scan.scanOnStartup();
      void settings.scan.refreshScanRoots().catch(() => {});
    },
    library,
    view,
    theme,
    openSettings: () => setView("settings"),
    closeSettings: () => {
      setView("gallery");
      setHighlightScan(false);
    },
    setTheme(next: AppTheme) {
      applyTheme(next);
      setTheme(next);
    }
  };
}
