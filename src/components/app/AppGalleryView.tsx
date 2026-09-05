import { lazy, Suspense } from "react";
import { TileSizeSlider } from "../common/TileSizeSlider";
import { GalleryGrid } from "../gallery/GalleryGrid";
import { TopBar } from "../topbar/TopBar";
import type {
  AppGalleryMediaController,
  AppGallerySearchController,
  BulkSelectionController
} from "./types";
import { LazyErrorBoundary } from "../UI/LazyErrorBoundary";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
import { useTranslation } from "react-i18next";

const BulkActionsSidebar = lazy(() =>
  import("../bulk/BulkActionsSidebar").then((module) => ({ default: module.BulkActionsSidebar }))
);

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
  const { t } = useTranslation();
  return (
    <>
      <TopBar
        toolbarContent={<>
          <span className="text-xs tabular-nums text-base-content/65" role="status">{t("workspace.itemCount", { count: media.assetCount })}</span>
      <TileSizeSlider
        tileSize={media.tileSize}
        min={media.tileSizeMin}
        max={media.tileSizeMax}
        step={media.tileSizeStep}
        onChange={media.onTileSizeChange}
        selectionModeEnabled={bulkSelection.selectionModeEnabled}
        onToggleSelectionMode={bulkSelection.onToggleSelectionMode}
      />
        </>}
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

      <div className="workspace-viewport">
        <section ref={media.scrollContainerRef} className="gallery-scroll h-full overflow-x-hidden overflow-y-auto">
          <div
            className={`workspace-body ${bulkSelection.selectionModeEnabled ? "workspace-body--bulk" : ""}`}
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
              loadError={media.loadError}
              onLoadRetry={media.onLoadRetry}
              pageFailureEpoch={media.pageFailureEpoch}
            />

            {bulkSelection.selectionModeEnabled ? (
              <LazyErrorBoundary
                resetKey={bulkSelection.selectionModeEnabled}
                fallback={
                  <aside className="m-4 grid content-start gap-3" aria-label={t("bulk.panel.ariaLabel")}>
                    <UiAlert tone="error" title={t("lazyLoad.bulkFailed")}>
                      {t("lazyLoad.retryDescription")}
                    </UiAlert>
                    <div className="flex gap-2">
                      <UiButton onClick={() => window.location.reload()}>{t("gallery.retry")}</UiButton>
                      <UiButton variant="ghost" onClick={bulkSelection.onToggleSelectionMode}>
                        {t("controls.disableBulkActions")}
                      </UiButton>
                    </div>
                  </aside>
                }
              >
                <Suspense fallback={<p className="p-4" role="status">{t("common.loading")}</p>}>
                  <BulkActionsSidebar
                    controller={bulkSelection}
                    thumbs={media.thumbs}
                    renderingThumbnailIds={media.renderingThumbnailIds}
                  />
                </Suspense>
              </LazyErrorBoundary>
            ) : null}
          </div>
        </section>
      </div>


    </>
  );
}
