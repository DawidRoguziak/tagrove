import { GroupOrderModal } from "./GroupOrderModal";
import { createEffect, createMemo, createSignal, onCleanup, For } from "solid-js";
import type { SuggestionClient } from "../search/suggestions/SuggestionClient";
import type { ThumbnailStore } from "../../library/thumbnails/thumbnailStore";
import { useLanguage } from "../../i18n/solid";
import type { BulkController } from "./createBulk";
import { TagEditor } from "../tagging/TagEditor";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiButton } from "../../ui/controls/UiButton";
import { UiIconButton } from "../../ui/controls/UiIconButton";
import { normalizeGroupKey } from "./bulkGroupOrderService";
import { normalizeBulkTag } from "./bulkTagMergeService";

interface BulkActionsSidebarProps {
  controller: BulkController;
  client: SuggestionClient;
  popularTags: string[];
  thumbnailStore: ThumbnailStore;
}

function generateUuid(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.randomUUID === "function"
  ) {
    return globalThis.crypto.randomUUID();
  }
  return `group-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
}

export function BulkActionsSidebar(props: BulkActionsSidebarProps) {
  const { t } = useLanguage();
  const [orderModalContext, setOrderModalContext] = createSignal<string | null>(null);
  const [orderModalActivated, setOrderModalActivated] = createSignal(false);
  const [tagDraft, setTagDraft] = createSignal("");
  const [tagInvalid, setTagInvalid] = createSignal(false);
  let tagInput: HTMLInputElement | undefined;
  let disposed = false;
  onCleanup(() => {
    disposed = true;
  });
  const [pendingTagFocus, setPendingTagFocus] = createSignal<ReadonlySet<number> | null>(null);
  const displayedTags = () =>
    props.controller.tagMode === "single"
      ? props.controller.singleAssetTags
      : props.controller.appliedBulkTags;
  const controlsDisabled = () =>
    props.controller.selectedAssetIds.size === 0 || props.controller.selectionBusy;
  const tagControlsDisabled = () =>
    controlsDisabled() ||
    props.controller.tagDetailsLoading ||
    props.controller.tagDetailsFailed ||
    props.controller.tagApplying;
  const normalizedGroupKey = () => normalizeGroupKey(props.controller.groupKeyDraft);
  const orderContext = () =>
    JSON.stringify([[...props.controller.selectedAssetIds], normalizedGroupKey()]);
  const selectedSummaryIds = createMemo(
    () => new Set(props.controller.selectedAssets.map((asset) => asset.id))
  );
  const hasOrderPanel = () =>
    Boolean(normalizedGroupKey()) &&
    props.controller.orderedAssetIds.filter((id) => selectedSummaryIds().has(id)).length > 1;
  createEffect(orderContext, (context) => {
    if (orderModalContext() !== context) setOrderModalContext(null);
  });
  let lastSelection: ReadonlySet<number> | undefined;
  createEffect(
    () => props.controller.selectedAssetIds,
    (ids) => {
      if (ids === lastSelection) return;
      lastSelection = ids;
      setTagDraft("");
      setTagInvalid(false);
      setPendingTagFocus(null);
    }
  );
  createEffect(
    () => ({
      pending: pendingTagFocus(),
      ids: props.controller.selectedAssetIds,
      disabled: tagControlsDisabled()
    }),
    ({ pending, ids, disabled }) => {
      if (!pending || pending !== ids || disabled) return;
      const frame = requestAnimationFrame(() => {
        if (pending !== props.controller.selectedAssetIds || tagControlsDisabled()) return;
        tagInput?.focus({ preventScroll: true });
        setPendingTagFocus(null);
      });
      return () => cancelAnimationFrame(frame);
    }
  );
  const addTagAndRestoreFocus = (tag: string) => {
    const attempted = normalizeBulkTag(tag);
    if (!attempted) {
      // Keep an invalid draft for correction instead of clearing it silently.
      setTagInvalid(tag.trim().length > 0);
      return;
    }
    setTagInvalid(false);
    setTagDraft(attempted);
    const selection = props.controller.selectedAssetIds;
    void props.controller
      .onAddTag(attempted)
      .then((saved) => {
        if (disposed || props.controller.selectedAssetIds !== selection) return;
        if (saved) setTagDraft("");
        setPendingTagFocus(selection);
      })
      .catch(() => {});
  };
  return (
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
            {t("bulk.panel.selectedItems", { count: props.controller.selectedAssetIds.size })}
          </span>
        </div>
        <UiIconButton
          icon="heart"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8!"
          active={props.controller.allSelectedFavorites}
          aria-pressed={props.controller.allSelectedFavorites ? "true" : "false"}
          aria-label={t(
            props.controller.allSelectedFavorites ? "bulk.favorite.remove" : "bulk.favorite.toggle"
          )}
          title={t(
            props.controller.allSelectedFavorites ? "bulk.favorite.remove" : "bulk.favorite.toggle"
          )}
          aria-busy={props.controller.favoriteApplying ? "true" : "false"}
          disabled={controlsDisabled() || props.controller.favoriteApplying}
          onClick={() => void props.controller.onToggleFavorite()}
        />
        {props.controller.selectedAssetIds.size > 0 ? (
          <div class="flex w-full flex-wrap items-center gap-x-3 gap-y-1">
            <UiButton
              className="h-8! min-h-8! text-xs"
              onClick={() => void props.controller.onBulkSelectionInteraction({ type: "clear" })}
            >
              {t("bulk.panel.clearSelection")}
            </UiButton>
            <span class="text-xs text-[var(--text-muted)]">
              {t("bulk.panel.clearSelectionHint")}
            </span>
          </div>
        ) : null}
        {props.controller.favoriteFailed ? (
          <UiAlert className="w-full" tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.favorite.failed")}
          </UiAlert>
        ) : null}
      </header>

      <section class="grid gap-3 bg-[var(--surface-solid)]" data-testid="bulk-group-panel">
        <div class="grid gap-0.5">
          <h3 class="m-0 text-sm">{t("bulk.panel.groupHeading")}</h3>
          <p class="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
            {t("bulk.panel.groupDescription")}
          </p>
        </div>
        <div class="grid grid-cols-[minmax(0,1fr)_40px] items-center gap-2">
          <input
            id="bulk-group-key-input"
            value={props.controller.groupKeyDraft}
            autocomplete="off"
            autocapitalize="none"
            autocorrect="off"
            spellcheck={false}
            onInput={(event) => props.controller.onGroupKeyDraftChange(event.currentTarget.value)}
            placeholder={t("bulk.groupModal.groupKeyPlaceholder")}
            aria-label={t("bulk.groupModal.groupKeyAria")}
            class="h-9 w-full"
            disabled={controlsDisabled() || props.controller.groupApplying}
          />
          <UiIconButton
            icon="reset"
            aria-label={t("bulk.groupModal.generateGroupAria")}
            title={t("bulk.groupModal.generateUuid")}
            onClick={() => props.controller.onGroupKeyDraftChange(generateUuid())}
            disabled={controlsDisabled() || props.controller.groupApplying}
          />
        </div>
        {props.controller.hasConflictingGroups ? (
          <UiAlert tone="warning" title={t("bulk.panel.groupConflictTitle")}>
            {t("bulk.panel.groupConflictDescription")}
          </UiAlert>
        ) : null}
        {props.controller.groupFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.groupSaveFailed")}
          </UiAlert>
        ) : null}
        {props.controller.metadataLoading ? <p role="status">{t("common.loading")}</p> : null}
        {props.controller.metadataFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.metadataFailed")}>
            <UiButton onClick={props.controller.onRetryMetadata}>{t("gallery.retry")}</UiButton>
          </UiAlert>
        ) : null}
        {props.controller.partialResult &&
        props.controller.partialResult.processed < props.controller.partialResult.requested ? (
          <p role="status">{t("bulk.panel.partialResult", props.controller.partialResult)}</p>
        ) : null}
        <UiButton
          variant={normalizedGroupKey() ? "primary" : "danger"}
          className="w-full justify-center"
          onClick={() => void props.controller.onApplyGroup().catch(() => {})}
          disabled={
            controlsDisabled() ||
            props.controller.groupApplying ||
            props.controller.metadataLoading ||
            props.controller.metadataFailed
          }
        >
          {props.controller.groupApplying
            ? t("bulk.groupModal.applyInProgress")
            : normalizedGroupKey()
              ? t("bulk.groupModal.applyGroup")
              : props.controller.selectedAssets.length === 1
                ? t("bulk.panel.removeGroup")
                : t("bulk.panel.removeGroups")}
        </UiButton>
      </section>

      {hasOrderPanel() ? (
        <section class="grid gap-2 bg-[var(--surface-solid)]" data-testid="bulk-group-order-panel">
          <div class="grid gap-0.5">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="m-0 text-sm">{t("bulk.panel.orderHeading")}</h3>
              <UiButton
                className="text-xs"
                data-testid="bulk-open-order-modal"
                disabled={
                  props.controller.selectionBusy ||
                  props.controller.groupApplying ||
                  props.controller.metadataLoading ||
                  props.controller.metadataFailed
                }
                onClick={() => {
                  setOrderModalActivated(true);
                  setOrderModalContext(orderContext());
                }}
              >
                {t("bulk.orderModal.open")}
              </UiButton>
            </div>
          </div>
        </section>
      ) : null}

      <section
        class="grid min-h-0 gap-3 overflow-visible bg-[var(--surface-solid)]"
        data-testid="bulk-tags-panel"
      >
        <div class="grid gap-0.5">
          <h3 class="m-0 text-sm">{t("bulk.panel.tagsHeading")}</h3>
          <p class="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
            {props.controller.tagMode === "multiple"
              ? t("bulk.panel.tagsMultipleDescription")
              : t("bulk.panel.tagsSingleDescription")}
          </p>
        </div>
        <TagEditor
          variant="bulk"
          tags={displayedTags()}
          loadingText={props.controller.tagDetailsLoading ? t("bulk.panel.loadingTags") : undefined}
          emptyText={
            props.controller.tagMode === "none" ? t("bulk.panel.selectItemsPrompt") : undefined
          }
          onRemoveTag={
            props.controller.tagMode === "single"
              ? (tag) => void props.controller.onRemoveTag(tag).catch(() => {})
              : undefined
          }
          getRemoveTagAriaLabel={(tag) => t("bulk.tagModal.removeTagAria", { tag })}
          draft={tagDraft()}
          onDraftChange={(value) => {
            setTagDraft(value);
            setTagInvalid(false);
          }}
          knownTags={props.controller.knownTags}
          onAddTag={addTagAndRestoreFocus}
          inputId="bulk-tag-draft-input"
          inputRef={(node) => {
            tagInput = node;
          }}
          client={props.client}
          placeholder={t("bulk.tagModal.tagInputPlaceholder")}
          inputAriaLabel={t("bulk.tagModal.addTag")}
          listboxAriaLabel={t("bulk.tagModal.suggestions")}
          disabled={tagControlsDisabled()}
        />
        {props.popularTags.length > 0 ? (
          <div class="grid gap-2" role="group" aria-label={t("bulk.panel.mostUsedTags")}>
            <p class="m-0 text-xs text-[var(--text-muted)]">{t("bulk.panel.mostUsedTags")}</p>
            <div class="flex flex-wrap gap-1.5">
              <For each={props.popularTags}>
                {(tag) => (
                  <UiButton
                    className="h-auto! min-h-7! max-w-full break-all px-2! py-1! text-xs"
                    disabled={tagControlsDisabled()}
                    onClick={() => addTagAndRestoreFocus(tag)}
                  >
                    {tag}
                  </UiButton>
                )}
              </For>
            </div>
          </div>
        ) : null}
        {tagInvalid() ? (
          <UiAlert tone="error" data-testid="bulk-tag-invalid">
            {t("search.errors.tagInvalidCharacters")}
          </UiAlert>
        ) : null}
        {props.controller.tagDetailsFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.tagLoadFailedTitle")}>
            <div class="grid gap-2">
              <span>{t("bulk.panel.tagLoadFailed")}</span>
              <UiButton className="btn-sm" onClick={props.controller.onRetryTagDetails}>
                {t("bulk.panel.retryTagDetails")}
              </UiButton>
            </div>
          </UiAlert>
        ) : null}
        {props.controller.tagSaveFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.tagSaveFailed")}
          </UiAlert>
        ) : null}
      </section>
      {orderModalActivated() && (
        <GroupOrderModal
          open={hasOrderPanel() && orderModalContext() === orderContext()}
          controller={props.controller}
          thumbnailStore={props.thumbnailStore}
          onClose={() => setOrderModalContext(null)}
        />
      )}
    </aside>
  );
}
