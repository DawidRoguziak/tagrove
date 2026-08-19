import { BulkActionsSidebar } from "../bulk/BulkActionsSidebar";
import { TileSizeSlider } from "../common/TileSizeSlider";
import { GalleryGrid } from "../gallery/GalleryGrid";
import { TopBar } from "../topbar/TopBar";
import type {
  AppGalleryMediaController,
  AppGallerySearchController,
  BulkSelectionController
} from "./types";

interface AppGalleryViewProps {
  search: AppGallerySearchController;
  media: AppGalleryMediaController;
  bulkSelection: BulkSelectionController;
  onOpenSettingsView: () => void;
}

const EMPTY_THUMBS: Record<number, string> = {};
const EMPTY_RENDERING_IDS: Record<number, true> = {};

export function AppGalleryView({
  search,
  media,
  bulkSelection,
  onOpenSettingsView,
}: AppGalleryViewProps) {
  return (
    <>
      <TopBar
        filterInput={search.filterInput}
        onFilterChange={search.onFilterChange}
        filterValidationError={search.filterValidationError}
        knownTags={search.knownTags}
        mediaKind={search.mediaKind}
        onMediaKindChange={search.onMediaKindChange}
        onSearchSubmit={search.onSearchSubmit}
        onApplyTagListSearch={search.onApplyTagListSearch}
        onClearSearch={search.onClearSearch}
        favoritesOnly={search.favoritesOnly}
        onFavoritesOnlyChange={search.onFavoritesOnlyChange}
        onOpenSettingsView={onOpenSettingsView}
      />

      <div
        className={`mx-auto grid min-h-[calc(100vh-4.5rem)] w-full ${
          bulkSelection.selectionModeEnabled
            ? "max-w-[2440px] grid-cols-[minmax(0,1fr)_clamp(28rem,33.6vw,33.6rem)]"
            : "max-w-[1920px] grid-cols-1"
        }`}
      >
        <GalleryGrid
          assets={media.assets}
          assetCount={media.assetCount}
          getAssetAt={media.getAssetAt}
          selectedId={media.selectedId}
          thumbs={EMPTY_THUMBS}
          tileSize={media.tileSize}
          hasMore={media.hasMore}
          isLoading={media.isLoading}
          isGeneratingThumbnails={media.isGeneratingThumbnails}
          pendingThumbnailCount={media.pendingThumbnailCount}
          renderingThumbnailIds={EMPTY_RENDERING_IDS}
          thumbnailStore={media.thumbnailStore}
          scrollContainerRef={media.scrollContainerRef}
          onReachEnd={media.onReachEnd}
          onVirtualRangeChange={media.onVirtualRangeChange}
          onCtrlWheelZoom={media.onCtrlWheelZoom}
          onSelect={media.onSelect}
          hasScanRoots={media.hasScanRoots}
          onAddFirstFolder={media.onAddFirstFolder}
          selectionModeEnabled={bulkSelection.selectionModeEnabled}
          selectedAssetIds={bulkSelection.selectedAssetIds}
          onBulkSelectionInteraction={bulkSelection.onBulkSelectionInteraction}
        />

        {bulkSelection.selectionModeEnabled ? (
          <BulkActionsSidebar
            controller={bulkSelection}
            thumbs={media.thumbs}
            renderingThumbnailIds={media.renderingThumbnailIds}
          />
        ) : null}
      </div>

      <TileSizeSlider
        tileSize={media.tileSize}
        min={media.tileSizeMin}
        max={media.tileSizeMax}
        step={media.tileSizeStep}
        onChange={media.onTileSizeChange}
        selectionModeEnabled={bulkSelection.selectionModeEnabled}
        onToggleSelectionMode={bulkSelection.onToggleSelectionMode}
      />
    </>
  );
}
