import { createLightbox } from "../features/lightbox/createLightbox";
import { createBulk } from "../features/bulk/createBulk";
import { getStartupPopularTags } from "../platform/tauri/tags";
import { createSearch } from "../features/search/createSearch";
import { createSelection } from "../features/gallery/createSelection";
import { SuggestionClient } from "../components/search/worker/SuggestionClient";
import { listScanRoots } from "../platform/tauri/scanning";
import {
  clampTileSize,
  DEFAULT_TILE_SIZE,
  getNextTileSizeByWheel
} from "../components/app/services/tileSizeService";
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
  const [hasScanRoots, setHasScanRoots] = createSignal(true);
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
    hasScanRoots,
    zoom: (delta: number) => setTileSize(getNextTileSizeByWheel(size, delta)),
    start() {
      if (started) return;
      started = true;
      void getStartupPopularTags()
        .then((tags) => {
          if (!disposed) setPopularTags(tags);
        })
        .catch(() => {});
      void library.refreshLibrary().catch(() => {});
      void listScanRoots()
        .then((roots) => {
          if (!disposed) setHasScanRoots(roots.length > 0);
        })
        .catch(() => {});
    },
    library,
    view,
    theme,
    openSettings: () => setView("settings"),
    closeSettings: () => setView("gallery"),
    setTheme(next: AppTheme) {
      applyTheme(next);
      setTheme(next);
    }
  };
}
