import { createEffect, createSignal, onCleanup } from "solid-js";
import { useLanguage } from "../../../i18n/solid";
import { UiButton } from "../../../ui/controls/UiButton";
import { UiIconButton } from "../../../ui/controls/UiIconButton";
import { UiModal } from "../../../ui/overlays/UiModal";
import { TagListChipList } from "./TagListChipList";
import { createTagList } from "./createTagList";
import {
  buildFilterInput,
  applySingleSelectionAction,
  applyDoubleSelectionAction,
  type TagSelections
} from "./tagListSelectionService";
const EMPTY_TAGS: string[] = [];
interface TagListModalProps {
  open: boolean;
  knownTags?: string[];
  onClose: () => void;
  onApplySearch?: (filterInput: string) => Promise<void> | void;
}
export function TagListModal(props: TagListModalProps) {
  const { t } = useLanguage();
  let input: HTMLInputElement | undefined;
  const [list, setList] = createSignal<HTMLDivElement>();
  const [query, setQuery] = createSignal("");
  const [isApplying, setApplying] = createSignal(false);
  let applying = false;
  let selections: TagSelections = { included: {}, excluded: {} };
  const [selection, publish] = createSignal(selections);
  const data = createTagList({
    open: () => props.open,
    tags: () => props.knownTags ?? EMPTY_TAGS,
    query,
    list
  });
  let pending: { tag: string; timer: number } | null = null;
  const update = (next: TagSelections) => {
    selections = next;
    publish(next);
  };
  const cancel = () => {
    if (pending) clearTimeout(pending.timer);
    pending = null;
  };
  const flush = () => {
    if (pending) {
      const tag = pending.tag;
      cancel();
      update(applySingleSelectionAction(selections, tag));
    }
  };
  function handleTagClick(tag: string, count: number) {
    if (count > 1 || applying) return;
    if (pending && pending.tag !== tag) flush();
    cancel();
    pending = { tag, timer: window.setTimeout(flush, 220) };
  }
  function handleTagDoubleClick(tag: string) {
    if (applying) return;
    cancel();
    update(applyDoubleSelectionAction(selections, tag));
  }
  let disposed = false;
  onCleanup(() => {
    disposed = true;
    cancel();
  });
  createEffect(
    () => props.open,
    (open) => {
      if (!open) {
        cancel();
        setQuery("");
        update({ included: {}, excluded: {} });
        return;
      }
      const frame = requestAnimationFrame(() => input?.focus());
      return () => cancelAnimationFrame(frame);
    }
  );
  async function handleApply() {
    if (!props.onApplySearch || applying) return;
    flush();
    applying = true;
    setApplying(true);
    try {
      await props.onApplySearch(buildFilterInput(selections));
      if (!disposed) props.onClose();
    } catch {
      /* Retain selection for explicit retry. */
    } finally {
      applying = false;
      if (!disposed) setApplying(false);
    }
  }
  function handleListScroll(event: Event & { currentTarget: HTMLDivElement }) {
    const node = event.currentTarget;
    if (node.scrollHeight - node.scrollTop - node.clientHeight <= 120) void data.loadMore();
  }
  const selectionSummary = () =>
    t("tagList.selectionSummary", {
      included: Object.keys(selection().included).length,
      excluded: Object.keys(selection().excluded).length
    });
  return (
    <UiModal
      open={props.open}
      onClose={props.onClose}
      closeOnOverlayClick={!isApplying()}
      closeOnEscape={!isApplying()}
      size="medium"
      labelledBy="tag-list-modal-heading"
      contentClassName="flex max-h-[calc(100vh-2rem)] flex-col gap-3 overflow-hidden"
      testId="tag-list-modal"
    >
      <div class="flex items-start justify-between gap-3">
        <h2 id="tag-list-modal-heading" class="m-0 text-lg">
          {t("tagList.modalTitle")}
        </h2>

        <UiIconButton
          icon="close"
          onClick={props.onClose}
          disabled={isApplying()}
          aria-label={t("tagList.closeModal")}
          title={t("tagList.closeModal")}
        />
      </div>

      <div class="grid gap-1.5">
        <label class="text-xs text-[var(--text-muted)]" for="tag-list-filter-input">
          {t("tagList.filterLabel")}
        </label>
        <input
          id="tag-list-filter-input"
          data-initial-focus
          ref={(node) => {
            input = node;
          }}
          value={query()}
          class="w-full rounded-[var(--radius-control)] border border-base-content/12 bg-base-100 px-3 py-2.5 text-sm text-base-content shadow-[var(--shadow-surface)] focus-visible:border-primary/40 focus-visible:outline-hidden focus-visible:shadow-[var(--field-shadow-focus)]"
          placeholder={t("tagList.filterPlaceholder")}
          aria-label={t("tagList.filterLabel")}
          disabled={isApplying()}
          onInput={(event) => {
            setQuery(event.currentTarget.value);
          }}
          autocomplete="off"
          autocorrect="off"
          autocapitalize="off"
          spellcheck={false}
        />
      </div>

      <p class="m-0 text-xs text-[var(--text-muted)]">{selectionSummary()}</p>

      <div
        ref={setList}
        class=" min-h-0 overflow-y-auto rounded-[var(--radius-control)]  p-2 "
        aria-label={t("tagList.listLabel")}
        onScroll={handleListScroll}
      >
        {data.loading && data.page.items.length === 0 ? (
          <p class="m-0 px-1 py-2 text-sm text-[var(--text-muted)]">{t("tagList.loading")}</p>
        ) : data.page.items.length === 0 ? (
          <p class="m-0 px-1 py-2 text-sm text-[var(--text-muted)]">
            {query().trim() ? t("tagList.noMatches") : t("tagList.empty")}
          </p>
        ) : (
          <TagListChipList
            tags={data.page.items}
            selections={selection()}
            disabled={isApplying()}
            onTagClick={handleTagClick}
            onTagDoubleClick={handleTagDoubleClick}
            getStateLabel={(tag) => {
              const included = Boolean(selection().included[tag.trim().toLowerCase()]);
              const excluded = Boolean(selection().excluded[tag.trim().toLowerCase()]);
              if (included) {
                return t("tagList.states.included");
              }
              if (excluded) {
                return t("tagList.states.excluded");
              }
              return t("tagList.states.inactive");
            }}
            getButtonTitle={(tag, state) => t("tagList.tagButtonTitle", { tag, state })}
            getButtonAriaLabel={(tag, state) => t("tagList.tagButtonAria", { tag, state })}
          />
        )}

        {data.loadingMore ? (
          <p class="m-0 px-1 py-2 text-xs text-[var(--text-muted)]">{t("tagList.loadingMore")}</p>
        ) : null}
      </div>

      <div class="flex items-center justify-end gap-2">
        <UiButton onClick={props.onClose} disabled={isApplying()}>
          {t("common.cancel")}
        </UiButton>
        <UiButton onClick={handleApply} disabled={isApplying() || !props.onApplySearch}>
          {isApplying() ? t("tagList.applying") : t("common.apply")}
        </UiButton>
      </div>
    </UiModal>
  );
}
