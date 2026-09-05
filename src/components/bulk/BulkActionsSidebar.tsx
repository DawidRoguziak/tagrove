import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toMediaSrc } from "../../api";
import type { BulkSelectionController } from "../app/types";
import { SearchTagator } from "../search/SearchTagator";
import { AssignedTagList } from "../UI/AssignedTagList";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../UI/ThumbnailImage";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
import { UiIcon } from "../UI/UiIcon";
import { UiIconButton } from "../UI/UiIconButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { normalizeGroupKey } from "./grouping/services/bulkGroupOrderService";
import { normalizeBulkTag } from "./tagging/services/bulkTagMergeService";

interface BulkActionsSidebarProps {
  controller: BulkSelectionController;
  thumbs: Record<number, string>;
  renderingThumbnailIds: Record<number, true>;
}

function generateUuid(): string {
  if (typeof globalThis.crypto !== "undefined" && typeof globalThis.crypto.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  return `group-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
}

export function BulkActionsSidebar({
  controller,
  thumbs,
  renderingThumbnailIds
}: BulkActionsSidebarProps) {
  const { t } = useTranslation();
  const [tagDraft, setTagDraft] = useState("");
  const [draggingAssetId, setDraggingAssetId] = useState<number | null>(null);
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const tagFocusFrameRef = useRef<number | null>(null);
  const draggingAssetIdRef = useRef<number | null>(null);
  const lastDragTargetAssetIdRef = useRef<number | null>(null);
  const selectionKey = controller.selectedAssets.map((asset) => asset.id).join(",");
  const orderedAssets = useMemo(() => {
    const selectedById = new Map(controller.selectedAssets.map((asset) => [asset.id, asset]));
    return controller.orderedAssetIds
      .map((assetId) => selectedById.get(assetId))
      .filter((asset): asset is NonNullable<typeof asset> => Boolean(asset));
  }, [controller.orderedAssetIds, controller.selectedAssets]);
  const displayedTags =
    controller.tagMode === "single" ? controller.singleAssetTags : controller.appliedBulkTags;
  const controlsDisabled = controller.selectedAssets.length === 0;
  const normalizedGroupKey = normalizeGroupKey(controller.groupKeyDraft);
  const hasOrderPanel = Boolean(normalizedGroupKey) && orderedAssets.length > 1;

  useEffect(() => {
    setTagDraft("");
    setDraggingAssetId(null);
    draggingAssetIdRef.current = null;
    lastDragTargetAssetIdRef.current = null;
  }, [selectionKey]);

  useEffect(() => {
    const finishDragging = () => {
      draggingAssetIdRef.current = null;
      lastDragTargetAssetIdRef.current = null;
      setDraggingAssetId(null);
    };

    window.addEventListener("pointerup", finishDragging);
    window.addEventListener("pointercancel", finishDragging);
    window.addEventListener("blur", finishDragging);
    return () => {
      window.removeEventListener("pointerup", finishDragging);
      window.removeEventListener("pointercancel", finishDragging);
      window.removeEventListener("blur", finishDragging);
      if (tagFocusFrameRef.current !== null) {
        cancelAnimationFrame(tagFocusFrameRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!controller.groupApplying) return;
    draggingAssetIdRef.current = null;
    lastDragTargetAssetIdRef.current = null;
    setDraggingAssetId(null);
  }, [controller.groupApplying]);

  const addTagAndRestoreFocus = (tag: string) => {
    const attemptedTag = normalizeBulkTag(tag);
    setTagDraft(attemptedTag);
    void controller.onAddTag(attemptedTag).then((saved) => {
      if (saved) setTagDraft("");
      if (tagFocusFrameRef.current !== null) {
        cancelAnimationFrame(tagFocusFrameRef.current);
      }
      tagFocusFrameRef.current = requestAnimationFrame(() => {
        tagFocusFrameRef.current = null;
        tagInputRef.current?.focus();
      });
    }).catch(() => {});
  };

  const submitTag = () => {
    addTagAndRestoreFocus(tagDraft);
  };

  return (
    <aside
      className="bulk-inspector panel-scroll"
      data-testid="bulk-action-panel"
      aria-label={t("bulk.panel.ariaLabel")}
    >
      <header
        className="flex flex-wrap items-center justify-between gap-2 bg-[var(--surface-solid)]"
        data-testid="bulk-header-panel"
      >
        <div className="grid gap-0.5" aria-live="polite">
          <h2 className="m-0 text-base">{t("bulk.panel.heading")}</h2>
          <span className="text-xs text-base-content/65">
            {t("bulk.panel.selectedItems", { count: controller.selectedAssetIds.size })}
          </span>
        </div>
        <UiIconButton
          icon="heart"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8!"
          active={controller.allSelectedFavorites}
          aria-pressed={controller.allSelectedFavorites}
          aria-label={t(controller.allSelectedFavorites ? "bulk.favorite.remove" : "bulk.favorite.toggle")}
          title={t(controller.allSelectedFavorites ? "bulk.favorite.remove" : "bulk.favorite.toggle")}
          aria-busy={controller.favoriteApplying}
          disabled={controller.selectedAssetIds.size === 0 || controller.favoriteApplying}
          onClick={() => void controller.onToggleFavorite()}
        />
        {controller.favoriteFailed ? (
          <UiAlert className="w-full" tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.favorite.failed")}
          </UiAlert>
        ) : null}
      </header>

      <section
        className="grid gap-3 bg-[var(--surface-solid)]"
        data-testid="bulk-group-panel"
      >
        <div className="grid gap-0.5">
          <h3 className="m-0 text-sm">{t("bulk.panel.groupHeading")}</h3>
          <p className="m-0 text-[11px] leading-relaxed text-base-content/60">
            {t("bulk.panel.groupDescription")}
          </p>
        </div>
        <div className="grid grid-cols-[minmax(0,1fr)_40px] items-center gap-2">
          <input
            id="bulk-group-key-input"
            value={controller.groupKeyDraft}
            {...browserAssistDisabledProps}
            onChange={(event) => controller.onGroupKeyDraftChange(event.target.value)}
            placeholder={t("bulk.groupModal.groupKeyPlaceholder")}
            aria-label={t("bulk.groupModal.groupKeyAria")}
            className="h-9 w-full"
            disabled={controlsDisabled || controller.groupApplying}
          />
          <UiIconButton
            icon="reset"
            aria-label={t("bulk.groupModal.generateGroupAria")}
            title={t("bulk.groupModal.generateUuid")}
            onClick={() => controller.onGroupKeyDraftChange(generateUuid())}
            disabled={controlsDisabled || controller.groupApplying}
          />
        </div>
        {controller.hasConflictingGroups ? (
          <UiAlert tone="warning" title={t("bulk.panel.groupConflictTitle")}>
            {t("bulk.panel.groupConflictDescription")}
          </UiAlert>
        ) : null}
        {controller.groupFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.groupSaveFailed")}
          </UiAlert>
        ) : null}
        <UiButton
          variant={normalizedGroupKey ? "primary" : "danger"}
          className="w-full justify-center"
          onClick={() => void controller.onApplyGroup().catch(() => {})}
          disabled={controlsDisabled || controller.groupApplying}
        >
          {controller.groupApplying
            ? t("bulk.groupModal.applyInProgress")
            : normalizedGroupKey
              ? t("bulk.groupModal.applyGroup")
              : controller.selectedAssets.length === 1
                ? t("bulk.panel.removeGroup")
                : t("bulk.panel.removeGroups")}
        </UiButton>
      </section>

      {hasOrderPanel ? (
        <section
          className="grid max-h-[300px] min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-2 overflow-hidden bg-[var(--surface-solid)]"
          data-testid="bulk-group-order-panel"
        >
          <div className="grid gap-0.5">
            <h3 className="m-0 text-sm">{t("bulk.panel.orderHeading")}</h3>
            <p className="m-0 text-[11px] leading-relaxed text-base-content/60">
              {t("bulk.panel.orderDescription")}
            </p>
          </div>
          <div
            className="panel-scroll grid min-h-0 gap-2 overflow-y-auto overscroll-contain"
            data-testid="bulk-group-order-list"
            role="list"
          >
            {orderedAssets.map((asset, index) => {
              const thumbPath = thumbs[asset.id];
              const src = thumbPath ? toMediaSrc(thumbPath) : TRANSPARENT_THUMBNAIL_SRC;
              const isRendering = Boolean(renderingThumbnailIds[asset.id]) && !thumbPath;
              return (
                <div
                  key={asset.id}
                  className={`flex items-center gap-2 rounded-[var(--radius-control)] border border-base-content/12 bg-base-100/70 p-1.5 transition-[opacity,box-shadow] ${
                    draggingAssetId === asset.id ? "opacity-60 ring-2 ring-primary/40" : ""
                  }`}
                  data-asset-id={asset.id}
                  data-testid={`bulk-group-tile-${asset.id}`}
                  role="listitem"
                  onPointerEnter={() => {
                    const draggedAssetId = draggingAssetIdRef.current;
                    if (
                      draggedAssetId === null ||
                      draggedAssetId === asset.id ||
                      lastDragTargetAssetIdRef.current === asset.id
                    ) {
                      return;
                    }
                    lastDragTargetAssetIdRef.current = asset.id;
                    controller.onReorderGroupAsset(draggedAssetId, asset.id);
                  }}
                >
                  <span
                    className="grid h-7 w-7 place-items-center rounded-[var(--radius-control)] bg-primary/14 text-xs font-semibold text-primary"
                    data-testid={`bulk-group-order-${asset.id}`}
                  >
                    {index + 1}
                  </span>
                  <div
                    className="relative h-16 w-20 shrink-0 overflow-hidden rounded-[var(--radius-control)] bg-base-200"
                    data-testid={`bulk-group-thumbnail-${asset.id}`}
                  >
                    <ThumbnailImage
                      src={src}
                      alt=""
                      className="h-full w-full object-cover"
                      draggable={false}
                    />
                    {isRendering ? (
                      <span className="absolute inset-0 grid place-items-center bg-base-100/55" aria-hidden="true">
                        <span className="h-4 w-4 animate-spin rounded-full border-2 border-base-content/25 border-t-primary" />
                      </span>
                    ) : null}
                  </div>
                  <span className="min-w-0 flex-1 truncate text-xs text-base-content/75" title={asset.file_name}>{asset.file_name}</span>
                  <button
                    type="button"
                    className={`grid h-10 w-10 shrink-0 touch-none select-none place-items-center rounded-[var(--radius-control)] text-base-content/55 transition-colors ${
                      controller.groupApplying
                        ? "cursor-not-allowed opacity-45"
                        : "cursor-grab hover:bg-base-content/8 hover:text-base-content active:cursor-grabbing"
                    }`}
                    data-testid={`bulk-group-drag-handle-${asset.id}`}
                    aria-label={t("bulk.panel.dragHandleAria", { position: index + 1 })}
                    aria-keyshortcuts="ArrowUp ArrowDown"
                    title={t("bulk.panel.dragHandleAria", { position: index + 1 })}
                    disabled={controller.groupApplying}
                    onKeyDown={(event) => {
                      if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                      event.preventDefault();
                      const targetIndex = event.key === "ArrowUp" ? index - 1 : index + 1;
                      const target = orderedAssets[targetIndex];
                      if (target) controller.onReorderGroupAsset(asset.id, target.id);
                    }}
                    onPointerDown={(event) => {
                      if (event.button > 0 || controller.groupApplying) return;
                      event.preventDefault();
                      draggingAssetIdRef.current = asset.id;
                      lastDragTargetAssetIdRef.current = null;
                      setDraggingAssetId(asset.id);
                    }}
                  >
                    <UiIcon name="grip-vertical" className="h-5 w-5" />
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      ) : null}

      <section
        className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)_auto_auto] gap-3 overflow-visible bg-[var(--surface-solid)]"
        data-testid="bulk-tags-panel"
      >
        <div className="grid gap-0.5">
          <h3 className="m-0 text-sm">{t("bulk.panel.tagsHeading")}</h3>
          <p className="m-0 text-[11px] leading-relaxed text-base-content/60">
            {controller.tagMode === "multiple"
              ? t("bulk.panel.tagsMultipleDescription")
              : t("bulk.panel.tagsSingleDescription")}
          </p>
        </div>
        <AssignedTagList
          variant="bulk"
          tags={displayedTags}
          loadingText={controller.tagDetailsLoading ? t("bulk.panel.loadingTags") : undefined}
          emptyText={
            controller.tagMode === "none"
              ? t("bulk.panel.selectItemsPrompt")
              : controller.tagMode === "multiple"
                ? t("bulk.panel.noBulkTagsAdded")
                : t("lightbox.noTags")
          }
          onRemoveTag={
            controller.tagMode === "single"
              ? (tag) => void controller.onRemoveTag(tag).catch(() => {})
              : undefined
          }
          getRemoveTagAriaLabel={(tag) => t("bulk.tagModal.removeTagAria", { tag })}
          removeDisabled={controller.tagApplying || controller.tagDetailsLoading || controller.tagDetailsFailed}
        />
        <SearchTagator
          inputId="bulk-tag-draft-input"
          value={tagDraft}
          onValueChange={setTagDraft}
          knownTags={controller.knownTags}
          excludedTags={displayedTags}
          onSuggestionPick={addTagAndRestoreFocus}
          onSubmit={submitTag}
          inputRef={tagInputRef}
          placeholder={t("bulk.tagModal.tagInputPlaceholder")}
          ariaLabel={t("bulk.tagModal.addTag")}
          listboxAriaLabel={t("bulk.tagModal.suggestions")}
          inputClassName="w-full"
          suggestionsStrategy="viewport"
          keepSuggestionsOpenOnPick
          autoSelectFirstSuggestion={false}
          disabled={controlsDisabled || controller.tagDetailsLoading || controller.tagDetailsFailed || controller.tagApplying}
        />
        {controller.tagDetailsFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.tagLoadFailedTitle")}>
            <div className="grid gap-2">
              <span>{t("bulk.panel.tagLoadFailed")}</span>
              <UiButton className="btn-sm" onClick={controller.onRetryTagDetails}>
                {t("bulk.panel.retryTagDetails")}
              </UiButton>
            </div>
          </UiAlert>
        ) : null}
        {controller.tagSaveFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.tagSaveFailed")}
          </UiAlert>
        ) : null}
      </section>
    </aside>
  );
}
