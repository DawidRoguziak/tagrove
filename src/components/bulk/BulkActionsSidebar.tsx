import { GroupOrderModal } from "./GroupOrderModal";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { BulkSelectionController } from "../app/types";
import { SearchTagator } from "../search/SearchTagator";
import { AssignedTagList } from "../UI/AssignedTagList";
import { ThumbnailSubscription } from "../UI/ThumbnailSubscription";
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
  const [orderModalContext, setOrderModalContext] = useState<string | null>(null);
  const [orderModalActivated, setOrderModalActivated] = useState(false);
  if (orderModalContext !== null && !orderModalActivated) setOrderModalActivated(true);
  const [tagDraft, setTagDraft] = useState("");
  const [draggingAssetId, setDraggingAssetId] = useState<number | null>(null);
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const tagFocusFrameRef = useRef<number | null>(null);
  const draggingAssetIdRef = useRef<number | null>(null);
  const lastDragTargetAssetIdRef = useRef<number | null>(null);
  const selectionKey = [...controller.selectedAssetIds].join(",");
  const orderedAssets = useMemo(() => {
    const selectedById = new Map(controller.selectedAssets.map((asset) => [asset.id, asset]));
    return controller.orderedAssetIds
      .map((assetId) => selectedById.get(assetId))
      .filter((asset): asset is NonNullable<typeof asset> => Boolean(asset));
  }, [controller.orderedAssetIds, controller.selectedAssets]);
  const displayedTags =
    controller.tagMode === "single" ? controller.singleAssetTags : controller.appliedBulkTags;
  const controlsDisabled = controller.selectedAssetIds.size === 0 || controller.selectionBusy;
  const tagControlsDisabled = controlsDisabled || controller.tagDetailsLoading || controller.tagDetailsFailed || controller.tagApplying;
  const normalizedGroupKey = normalizeGroupKey(controller.groupKeyDraft);
  const orderContext = JSON.stringify([selectionKey, normalizedGroupKey]);
  const hasOrderPanel = Boolean(normalizedGroupKey) && orderedAssets.length > 1;

  useEffect(() => {
    setOrderModalContext(previous => previous === orderContext ? previous : null);
  }, [orderContext]);

  const orderScrollRef = useRef<HTMLDivElement | null>(null);
  const orderVirtualizer = useVirtualizer({ count: hasOrderPanel ? orderedAssets.length : 0,
    getItemKey: index => orderedAssets[index]?.id ?? index,
    getScrollElement: () => orderScrollRef.current, estimateSize: () => 86, overscan: 3,
    initialRect: { width: 320, height: 240 } });
  const rows = orderVirtualizer.getVirtualItems();
  const firstRow = rows[0]?.index ?? 0;
  const lastRow = rows.at(-1)?.index ?? -1;
  useEffect(() => {
    if (hasOrderPanel) controller.queueThumbnailsByIds?.(orderedAssets.slice(firstRow, lastRow + 1).map(asset => asset.id));
  }, [controller.queueThumbnailsByIds, orderedAssets, hasOrderPanel, firstRow, lastRow]);
  const selectionKeyRef = useRef(selectionKey);
  selectionKeyRef.current = selectionKey;

  // biome-ignore lint/correctness/useExhaustiveDependencies: selected ID identity resets the editor draft.
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
    const capturedSelection = selectionKey;
    void controller.onAddTag(attemptedTag).then((saved) => {
      if (selectionKeyRef.current !== capturedSelection) return;
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
          <h2 className="m-0 text-lg">{t("bulk.panel.heading")}</h2>
          <span className="font-mono text-xs text-primary-text">
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
          disabled={controlsDisabled || controller.favoriteApplying}
          onClick={() => void controller.onToggleFavorite()}
        />
        {controller.selectedAssetIds.size > 0 ? (
          <div className="flex w-full flex-wrap items-center gap-x-3 gap-y-1">
            <UiButton
              className="h-8! min-h-8! text-xs"
              onClick={() => void controller.onBulkSelectionInteraction({ type: "clear" })}
            >
              {t("bulk.panel.clearSelection")}
            </UiButton>
            <span className="text-xs text-[var(--text-muted)]">
              {t("bulk.panel.clearSelectionHint")}
            </span>
          </div>
        ) : null}
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
          <p className="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
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
        {controller.metadataLoading ? <p role="status">{t("common.loading")}</p> : null}
        {controller.metadataFailed ? <UiAlert tone="error" title={t("bulk.panel.metadataFailed")}><UiButton onClick={controller.onRetryMetadata}>{t("gallery.retry")}</UiButton></UiAlert> : null}
        {controller.partialResult && controller.partialResult.processed < controller.partialResult.requested ? <p role="status">{t("bulk.panel.partialResult", controller.partialResult)}</p> : null}
        <UiButton
          variant={normalizedGroupKey ? "primary" : "danger"}
          className="w-full justify-center"
          onClick={() => void controller.onApplyGroup().catch(() => {})}
          disabled={controlsDisabled || controller.groupApplying || controller.metadataLoading || controller.metadataFailed}
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
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 className="m-0 text-sm">{t("bulk.panel.orderHeading")}</h3>
              <UiButton className="text-xs" data-testid="bulk-open-order-modal"
                disabled={controller.selectionBusy || controller.groupApplying || controller.metadataLoading || controller.metadataFailed}
                onClick={() => setOrderModalContext(orderContext)}>{t("bulk.orderModal.open")}</UiButton>
            </div>
            <p className="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
              {t("bulk.panel.orderDescription")}
            </p>
          </div>
          <div
            ref={orderScrollRef}
            style={{ height: Math.min(240, orderedAssets.length * 86) }}
            className="panel-scroll relative min-h-0 overflow-y-auto overscroll-contain"
            data-testid="bulk-group-order-list"
            role="list"
          >
            <div style={{ height: orderVirtualizer.getTotalSize(), position: "relative" }}>
            {rows.map(row => {
              const index = row.index;
              const asset = orderedAssets[index]!;
              return (
                <div
                  key={asset.id}
                  style={{ position: "absolute", top: 0, left: 0, width: "100%", height: 78, transform: `translateY(${row.start}px)` }}
                  className={`flex items-center gap-2 rounded-[var(--radius-control)] border border-base-content/12 bg-base-100/70 p-1.5 motion-feedback-opacity ${
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
                    className="grid h-7 w-7 place-items-center rounded-[var(--radius-control)] bg-primary/14 text-xs font-semibold text-primary-text"
                    data-testid={`bulk-group-order-${asset.id}`}
                  >
                    {index + 1}
                  </span>
                  <div
                    className="relative h-16 w-20 shrink-0 overflow-hidden rounded-[var(--radius-control)] bg-base-200"
                    data-testid={`bulk-group-thumbnail-${asset.id}`}
                  >
                    <ThumbnailSubscription id={asset.id} path={thumbs[asset.id]} rendering={Boolean(renderingThumbnailIds[asset.id])} />
                  </div>
                  <span className="min-w-0 flex-1 truncate text-xs text-[var(--text-muted)]" title={asset.file_name}>{asset.file_name}</span>
                  <button
                    type="button"
                    className={`grid h-10 w-10 shrink-0 touch-none select-none place-items-center rounded-[var(--radius-control)] text-[var(--text-muted)] ${
                      controller.groupApplying
                        ? "cursor-not-allowed opacity-45"
                        : "cursor-grab hover:bg-base-content/8 hover:text-base-content active:cursor-grabbing"
                    }`}
                    data-testid={`bulk-group-drag-handle-${asset.id}`}
                    aria-label={t("bulk.panel.dragHandleAria", { position: index + 1 })}
                    aria-keyshortcuts="ArrowUp ArrowDown"
                    title={t("bulk.panel.dragHandleAria", { position: index + 1 })}
                    disabled={controller.selectionBusy || controller.groupApplying}
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
          </div>
        </section>
      ) : null}

      <section
        className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)_auto_auto] gap-3 overflow-visible bg-[var(--surface-solid)]"
        data-testid="bulk-tags-panel"
      >
        <div className="grid gap-0.5">
          <h3 className="m-0 text-sm">{t("bulk.panel.tagsHeading")}</h3>
          <p className="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
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
          removeDisabled={controller.selectionBusy || controller.tagApplying || controller.tagDetailsLoading || controller.tagDetailsFailed}
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
          disabled={tagControlsDisabled}
        />
        {controller.startupPopularTags.length > 0 ? (
          <div className="grid gap-2" role="group" aria-label={t("bulk.panel.mostUsedTags")}>
            <p className="m-0 text-xs text-[var(--text-muted)]">{t("bulk.panel.mostUsedTags")}</p>
            <div className="flex flex-wrap gap-1.5">
              {controller.startupPopularTags.map((tag) => (
                <UiButton
                  key={tag}
                  className="h-auto! min-h-7! max-w-full break-all px-2! py-1! text-xs"
                  disabled={tagControlsDisabled}
                  onClick={() => addTagAndRestoreFocus(tag)}
                >
                  {tag}
                </UiButton>
              ))}
            </div>
          </div>
        ) : null}
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
      {orderModalActivated && <GroupOrderModal open={hasOrderPanel && orderModalContext === orderContext} controller={controller} thumbs={thumbs}
          renderingThumbnailIds={renderingThumbnailIds} onClose={() => setOrderModalContext(null)} />}
    </aside>
  );
}
