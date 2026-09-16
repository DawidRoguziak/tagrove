import { GroupOrderModal } from "./GroupOrderModal";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { BulkSelectionController } from "../app/types";
import { SearchTagator } from "../search/SearchTagator";
import { AssignedTagList } from "../UI/AssignedTagList";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
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
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const tagFocusFrameRef = useRef<number | null>(null);
  const selectionKey = [...controller.selectedAssetIds].join(",");
  const displayedTags =
    controller.tagMode === "single" ? controller.singleAssetTags : controller.appliedBulkTags;
  const controlsDisabled = controller.selectedAssetIds.size === 0 || controller.selectionBusy;
  const tagControlsDisabled = controlsDisabled || controller.tagDetailsLoading || controller.tagDetailsFailed || controller.tagApplying;
  const normalizedGroupKey = normalizeGroupKey(controller.groupKeyDraft);
  const orderContext = JSON.stringify([selectionKey, normalizedGroupKey]);
  const selectedSummaryIds = new Set(controller.selectedAssets.map(asset => asset.id));
  const hasOrderPanel = Boolean(normalizedGroupKey) && controller.orderedAssetIds.filter(id => selectedSummaryIds.has(id)).length > 1;

  useEffect(() => {
    setOrderModalContext(previous => previous === orderContext ? previous : null);
  }, [orderContext]);

  const selectionKeyRef = useRef(selectionKey);
  selectionKeyRef.current = selectionKey;

  // biome-ignore lint/correctness/useExhaustiveDependencies: selected ID identity resets the editor draft.
  useEffect(() => {
    setTagDraft("");
  }, [selectionKey]);

  useEffect(() => () => {
    if (tagFocusFrameRef.current !== null) cancelAnimationFrame(tagFocusFrameRef.current);
  }, []);

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
          className="grid gap-2 bg-[var(--surface-solid)]"
          data-testid="bulk-group-order-panel"
        >
          <div className="grid gap-0.5">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 className="m-0 text-sm">{t("bulk.panel.orderHeading")}</h3>
              <UiButton className="text-xs" data-testid="bulk-open-order-modal"
                disabled={controller.selectionBusy || controller.groupApplying || controller.metadataLoading || controller.metadataFailed}
                onClick={() => setOrderModalContext(orderContext)}>{t("bulk.orderModal.open")}</UiButton>
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
