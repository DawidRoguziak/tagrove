import { GroupOrderModal } from "./GroupOrderModal";
import { GroupOrderBoard } from "./GroupOrderBoard";
import { createEffect, createMemo, createSignal, createUniqueId, onCleanup, For } from "solid-js";
import type { SuggestionClient } from "../search/suggestions/SuggestionClient";
import { useLanguage } from "../../i18n/solid";
import type { BulkController } from "./createBulk";
import { TagEditor } from "../tagging/TagEditor";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiButton } from "../../ui/controls/UiButton";
import { UiIcon } from "../../ui/controls/UiIcon";
import { UiIconButton } from "../../ui/controls/UiIconButton";
import { normalizeGroupKey } from "./bulkGroupOrderService";
import { normalizeBulkTag } from "./bulkTagMergeService";

interface BulkDockProps {
  controller: BulkController;
  client: SuggestionClient;
}

type BulkDockTab = "tags" | "group";
const TABS: readonly BulkDockTab[] = ["tags", "group"];

function generateUuid(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.randomUUID === "function"
  ) {
    return globalThis.crypto.randomUUID();
  }
  return `group-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
}

const sameOrder = (left: readonly number[], right: readonly number[]) =>
  left.length === right.length && left.every((id, index) => id === right[index]);

/**
 * Floating bulk-actions dock at the bottom centre of the gallery. It stays a small chip until
 * something is selected, then shows the tag input with favorite and clear-selection controls
 * and two tabs: tags (compact) and group & order (tall, with inline drag-and-drop sorting).
 */
export function BulkDock(props: BulkDockProps) {
  const { t } = useLanguage();
  const tabIds = { tags: createUniqueId(), group: createUniqueId() };
  const panelIds = { tags: createUniqueId(), group: createUniqueId() };
  const [tab, setTab] = createSignal<BulkDockTab>("tags");
  const [orderModalContext, setOrderModalContext] = createSignal<string | null>(null);
  const [orderModalActivated, setOrderModalActivated] = createSignal(false);
  const [inlineOrder, setInlineOrder] = createSignal<number[]>([]);
  const [tagDraft, setTagDraft] = createSignal("");
  const [tagInvalid, setTagInvalid] = createSignal(false);
  let tagInput: HTMLInputElement | undefined;
  const tabButtons: Partial<Record<BulkDockTab, HTMLButtonElement>> = {};
  let disposed = false;
  onCleanup(() => {
    disposed = true;
  });
  const [pendingTagFocus, setPendingTagFocus] = createSignal<object | null>(null);
  const expanded = () =>
    props.controller.selectedCount > 0 || props.controller.selectionNotice !== null;
  const displayedTags = () =>
    props.controller.tagMode === "single"
      ? props.controller.singleAssetTags
      : props.controller.appliedBulkTags;
  const controlsDisabled = () =>
    props.controller.selectedCount === 0 || props.controller.selectionBusy;
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
  const orderModalOpen = () => hasOrderPanel() && orderModalContext() === orderContext();
  const orderLocked = () => props.controller.selectionBusy || props.controller.groupApplying;
  createEffect(orderContext, (context) => {
    if (orderModalContext() !== context) setOrderModalContext(null);
  });
  // The inline draft restarts from the controller's order for each selection, key and save.
  createEffect(
    () => ({ context: orderContext(), order: props.controller.orderedAssetIds }),
    ({ order }) => {
      setInlineOrder([...order]);
    }
  );
  createEffect(
    () => props.controller.selectedCount === 0,
    (empty) => {
      if (empty) setTab("tags");
    }
  );
  let lastSelection: object | undefined;
  createEffect(
    () => props.controller.selectionKey,
    (key) => {
      if (key === lastSelection) return;
      lastSelection = key;
      setTagDraft("");
      setTagInvalid(false);
      setPendingTagFocus(null);
    }
  );
  createEffect(
    () => ({
      pending: pendingTagFocus(),
      key: props.controller.selectionKey,
      disabled: tagControlsDisabled()
    }),
    ({ pending, key, disabled }) => {
      if (!pending || pending !== key || disabled) return;
      const frame = requestAnimationFrame(() => {
        if (pending !== props.controller.selectionKey || tagControlsDisabled()) return;
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
    const selection = props.controller.selectionKey;
    void props.controller
      .onAddTag(attempted)
      .then((saved) => {
        if (disposed || props.controller.selectionKey !== selection) return;
        if (saved) setTagDraft("");
        setPendingTagFocus(selection);
      })
      .catch(() => {});
  };
  const applyGroup = () => {
    // Apply the inline order only when the user changed it; the controller owns the default.
    const order =
      hasOrderPanel() && !sameOrder(inlineOrder(), props.controller.orderedAssetIds)
        ? inlineOrder()
        : undefined;
    void props.controller.onApplyGroup(order).catch(() => {});
  };
  const selectTab = (next: BulkDockTab, focus = false) => {
    setTab(next);
    if (focus) tabButtons[next]?.focus();
  };
  const tabLabel = (value: BulkDockTab) =>
    t(value === "tags" ? "bulk.dock.tabTags" : "bulk.dock.tabGroup");

  return (
    <aside
      class={`bulk-dock ${expanded() ? "bulk-dock--expanded" : ""} ${expanded() && tab() === "group" ? "bulk-dock--tall" : ""}`}
      data-testid="bulk-action-panel"
      data-selected-count={props.controller.selectedCount}
      aria-label={t("bulk.panel.ariaLabel")}
    >
      {expanded() ? null : (
        <div class="bulk-dock-chip" data-testid="bulk-dock-chip" aria-live="polite">
          <UiIcon name="bulk-actions" className="h-4 w-4 shrink-0" />
          <span class="truncate">{t("bulk.dock.collapsedHint")}</span>
          <span class="sr-only">{t("bulk.panel.selectedItems", { count: 0 })}</span>
        </div>
      )}
      {expanded() ? (
        <>
          {props.controller.selectionNotice ? (
            <UiAlert
              tone="warning"
              title={t("bulk.panel.selectionClearedTitle")}
              data-testid="bulk-selection-cleared"
            >
              {t(
                props.controller.selectionNotice === "rangeExpired"
                  ? "bulk.panel.selectionExpiredDescription"
                  : "bulk.panel.selectionClearedDescription"
              )}
            </UiAlert>
          ) : null}
          <header
            class="grid grid-cols-[minmax(0,1fr)_auto_auto] items-start gap-2"
            data-testid="bulk-header-panel"
          >
            <TagEditor
              variant="bulk"
              tags={displayedTags()}
              loadingText={
                props.controller.tagDetailsLoading ? t("bulk.panel.loadingTags") : undefined
              }
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
              suggestionsPlacement="above"
              disabled={tagControlsDisabled()}
            />
            <UiIconButton
              icon="heart"
              iconClassName="h-4 w-4 shrink-0"
              className="h-[50px]! w-10! min-h-[50px]!"
              active={props.controller.allSelectedFavorites}
              aria-pressed={props.controller.allSelectedFavorites ? "true" : "false"}
              aria-label={t(
                props.controller.allSelectedFavorites
                  ? "bulk.favorite.remove"
                  : "bulk.favorite.toggle"
              )}
              title={t(
                props.controller.allSelectedFavorites
                  ? "bulk.favorite.remove"
                  : "bulk.favorite.toggle"
              )}
              aria-busy={props.controller.favoriteApplying ? "true" : "false"}
              disabled={controlsDisabled() || props.controller.favoriteApplying}
              onClick={() => void props.controller.onToggleFavorite()}
            />
            <button
              type="button"
              class="bulk-dock-count"
              data-testid="bulk-clear-selection"
              aria-label={t("bulk.dock.clearSelectionAria", {
                count: props.controller.selectedCount
              })}
              title={t("bulk.dock.clearSelectionAria", { count: props.controller.selectedCount })}
              disabled={props.controller.selectedCount === 0}
              onClick={() => void props.controller.onBulkSelectionInteraction({ type: "clear" })}
            >
              <span class="tabular-nums" aria-live="polite">
                {props.controller.selectedCount}
              </span>
              <UiIcon name="close" className="h-3.5 w-3.5 [stroke-width:2.4]" />
            </button>
          </header>
          {props.controller.favoriteFailed ? (
            <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
              {t("bulk.favorite.failed")}
            </UiAlert>
          ) : null}
          {/* Header writes report here so their feedback is visible on either tab. */}
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
          {props.controller.partialResult &&
          props.controller.partialResult.processed <
            props.controller.partialResult.requested ? (
            <p role="status">{t("bulk.panel.partialResult", props.controller.partialResult)}</p>
          ) : null}
          <div class="bulk-dock-tabs" role="tablist" aria-label={t("bulk.panel.ariaLabel")}>
            <For each={TABS}>
              {(value) => (
                <button
                  type="button"
                  role="tab"
                  ref={(node) => {
                    tabButtons[value] = node;
                  }}
                  id={tabIds[value]}
                  aria-controls={panelIds[value]}
                  aria-selected={tab() === value ? "true" : "false"}
                  tabindex={tab() === value ? 0 : -1}
                  data-testid={`bulk-tab-${value}`}
                  class="bulk-dock-tab"
                  onClick={() => selectTab(value)}
                  onKeyDown={(event) => {
                    const index = TABS.indexOf(value);
                    const next =
                      event.key === "ArrowRight"
                        ? TABS[(index + 1) % TABS.length]
                        : event.key === "ArrowLeft"
                          ? TABS[(index + TABS.length - 1) % TABS.length]
                          : event.key === "Home"
                            ? TABS[0]
                            : event.key === "End"
                              ? TABS[TABS.length - 1]
                              : undefined;
                    if (!next) return;
                    event.preventDefault();
                    selectTab(next, true);
                  }}
                >
                  {tabLabel(value)}
                </button>
              )}
            </For>
          </div>
          {tab() === "tags" ? (
            <section
              class="grid gap-2 motion-enter"
              role="tabpanel"
              id={panelIds.tags}
              aria-labelledby={tabIds.tags}
              data-testid="bulk-tags-panel"
            >
              <p class="m-0 text-xs leading-relaxed text-[var(--text-muted)]">
                {props.controller.tagMode === "multiple"
                  ? t("bulk.panel.tagsMultipleDescription")
                  : t("bulk.panel.tagsSingleDescription")}
              </p>
            </section>
          ) : (
            <section
              class="flex min-h-0 flex-1 flex-col gap-3 motion-enter"
              role="tabpanel"
              id={panelIds.group}
              aria-labelledby={tabIds.group}
              data-testid="bulk-group-panel"
            >
              <div class="grid shrink-0 grid-cols-[minmax(0,1fr)_40px_auto] items-center gap-2">
                <input
                  id="bulk-group-key-input"
                  value={props.controller.groupKeyDraft}
                  autocomplete="off"
                  autocapitalize="none"
                  autocorrect="off"
                  spellcheck={false}
                  onInput={(event) =>
                    props.controller.onGroupKeyDraftChange(event.currentTarget.value)
                  }
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
                <UiButton
                  variant={normalizedGroupKey() ? "primary" : "danger"}
                  className="justify-center"
                  data-testid="bulk-apply-group"
                  onClick={applyGroup}
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
              {props.controller.metadataLoading ? (
                <p role="status">{t("common.loading")}</p>
              ) : null}
              {props.controller.metadataFailed ? (
                <UiAlert tone="error" title={t("bulk.panel.metadataFailed")}>
                  <UiButton onClick={props.controller.onRetryMetadata}>
                    {t("gallery.retry")}
                  </UiButton>
                </UiAlert>
              ) : null}
              {hasOrderPanel() ? (
                // The panel keeps room for a row of tiles; when alerts take the rest of the tall
                // dock, the dock scrolls instead of squeezing the board to nothing.
                <div
                  class="flex min-h-40 flex-1 flex-col gap-2"
                  data-testid="bulk-group-order-panel"
                >
                  <div class="flex shrink-0 flex-wrap items-center justify-between gap-2">
                    <div class="grid gap-0.5">
                      <h3 class="m-0 text-sm">{t("bulk.panel.orderHeading")}</h3>
                      <p class="m-0 text-xs text-[var(--text-muted)]">
                        {t("bulk.panel.orderDescription")}
                      </p>
                    </div>
                    <UiButton
                      className="text-xs"
                      data-testid="bulk-open-order-modal"
                      disabled={
                        orderLocked() ||
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
                  {orderModalOpen() ? (
                    <div class="min-h-0 flex-1" />
                  ) : (
                    <GroupOrderBoard
                      active
                      order={inlineOrder()}
                      onOrderChange={setInlineOrder}
                      assets={props.controller.selectedAssets}
                      locked={orderLocked()}
                      minTileWidth={150}
                      testIdPrefix="bulk-inline-order"
                      class="flex-1"
                    />
                  )}
                </div>
              ) : null}
            </section>
          )}
        </>
      ) : null}
      {orderModalActivated() && (
        <GroupOrderModal
          open={orderModalOpen()}
          controller={props.controller}
          initialOrder={inlineOrder()}
          onClose={() => setOrderModalContext(null)}
        />
      )}
    </aside>
  );
}
