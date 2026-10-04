import { UiButton } from "../ui/controls/UiButton";
import { createSignal } from "solid-js";
import { useAppRuntime } from "./AppRuntimeContext";
import { useLanguage } from "../i18n/solid";
import { SearchTagatorWrapper } from "../features/search/SearchTagatorWrapper";
import { GalleryGrid } from "../features/gallery/GalleryGrid";
import { TileSizeSlider } from "../features/gallery/TileSizeSlider";
import { UiIconButton } from "../ui/controls/UiIconButton";
import { WindowControls } from "./WindowControls";
import {
  TILE_SIZE_MIN,
  TILE_SIZE_MAX,
  TILE_SIZE_STEP
} from "../components/app/services/tileSizeService";
export function AppGalleryView() {
  const runtime = useAppRuntime();
  const { search, library, selection } = runtime;
  const { t } = useLanguage();
  const [scroller, setScroller] = createSignal<HTMLElement>();
  const retry = () => {
    library.query.retryLoad();
    void selection.retry();
  };
  return (
    <>
      <header class="workspace-header">
        <SearchTagatorWrapper
          client={runtime.suggestions}
          identity={
            <div class="workspace-identity">
              <img
                src="/tagrove.svg"
                alt="Tagrove"
                width={32}
                height={32}
                class="shrink-0"
                draggable={false}
              />
              <span aria-hidden="true">Tagrove</span>
            </div>
          }
          headerAction={
            <div class="flex items-center gap-3">
              <UiIconButton
                id="open-settings-button"
                icon="settings"
                onClick={runtime.openSettings}
                aria-label={t("topBar.openSettings")}
                title={t("topBar.openSettings")}
              />
              <WindowControls />
            </div>
          }
          toolbarContent={
            <>
              <span class="workspace-count" role="status">
                {t("workspace.itemCount", { count: library.query.total })}
              </span>
              <TileSizeSlider
                tileSize={runtime.tileSize()}
                min={TILE_SIZE_MIN}
                max={TILE_SIZE_MAX}
                step={TILE_SIZE_STEP}
                onChange={runtime.setTileSize}
                selectionModeEnabled={selection.enabled}
                onToggleSelectionMode={selection.toggle}
              />
            </>
          }
          filterInput={search.filterInput}
          onFilterChange={search.change}
          validationError={search.validationError}
          knownTags={library.vocabulary.knownTags}
          mediaKind={search.mediaKind}
          onMediaKindChange={search.applyKind}
          onSearchSubmit={() => {
            void search.submit().catch(() => {});
          }}
          onApplyTagListSearch={search.applyInput}
          onClearAll={() => {
            void search.clear().catch(() => {});
          }}
          favoritesOnly={search.favoritesOnly}
          onFavoritesOnlyChange={search.applyFavorites}
        />
      </header>
      <div class="workspace-viewport">
        <section ref={setScroller} class="gallery-scroll h-full overflow-x-hidden overflow-y-auto">
          <div class={`workspace-body ${selection.enabled ? "workspace-body--bulk" : ""}`}>
            <GalleryGrid
              assets={library.query.assets}
              assetCount={library.query.total}
              getAssetAt={library.query.getAssetAt}
              tileSize={runtime.tileSize()}
              hasMore={library.query.offset < library.query.total}
              isLoading={library.query.loading}
              isGeneratingThumbnails={library.thumbnails.isGeneratingPage}
              pendingThumbnailCount={library.thumbnails.pendingPageSize}
              thumbnailStore={library.thumbnails.thumbnailStore}
              scroller={scroller}
              onReachEnd={library.query.handleReachEnd}
              onVirtualRangeChange={library.handleVirtualRangeChange}
              onCtrlWheelZoom={runtime.zoom}
              onSelect={() => {}}
              hasScanRoots={runtime.hasScanRoots()}
              onAddFirstFolder={runtime.openSettings}
              selectionModeEnabled={selection.enabled}
              selectedAssetIds={selection.ids}
              onBulkSelectionInteraction={selection.interact}
              loadError={selection.error ?? library.query.loadError}
              onLoadRetry={retry}
              queryEpoch={library.query.queryEpoch}
              pageFailureEpoch={library.query.pageFailureEpoch}
            />
            {selection.enabled ? (
              <aside
                class="bulk-inspector panel-scroll"
                data-testid="bulk-action-panel"
                aria-label={t("bulk.panel.ariaLabel")}
              >
                <header
                  class="flex flex-wrap items-center justify-between gap-2 bg-[var(--surface-solid)]"
                  data-testid="bulk-header-panel"
                >
                  <div class="grid gap-0.5" aria-live="polite">
                    <h2 class="m-0 text-lg">{t("bulk.panel.heading")}</h2>
                    <span class="font-mono text-xs text-primary-text">
                      {t("bulk.panel.selectedItems", { count: selection.ids.size })}
                    </span>
                  </div>
                  {selection.ids.size > 0 ? (
                    <div class="flex w-full flex-wrap items-center gap-x-3 gap-y-1">
                      <UiButton className="h-8! min-h-8! text-xs" onClick={selection.clear}>
                        {t("bulk.panel.clearSelection")}
                      </UiButton>
                      <span class="text-xs text-[var(--text-muted)]">
                        {t("bulk.panel.clearSelectionHint")}
                      </span>
                    </div>
                  ) : null}
                </header>
              </aside>
            ) : null}
          </div>
        </section>
      </div>
    </>
  );
}
