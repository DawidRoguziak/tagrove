import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import type { UIEvent } from "react";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { UiButton } from "../UI/UiButton";
import { UiIconButton } from "../UI/UiIconButton";
import { UiModal } from "../UI/UiModal";
import { TagListChipList } from "./components/TagListChipList";
import { useTagListData } from "./hooks/useTagListData";
import { useTagSelectionIntent } from "./hooks/useTagSelectionIntent";
import { useTranslation } from "react-i18next";
import {
  buildFilterInput,
  type TagSelections
} from "./services/tagListSelectionService";

const PAGE_SIZE = 100;
const LOAD_MORE_THRESHOLD_PX = 120;
const SINGLE_CLICK_DELAY_MS = 220;
const EMPTY_TAGS: string[] = [];

interface TagListModalProps {
  open: boolean;
  knownTags?: string[];
  onClose: () => void;
  onApplySearch?: (filterInput: string) => Promise<void> | void;
}


export function TagListModal({
  open,
  knownTags = EMPTY_TAGS,
  onClose,
  onApplySearch
}: TagListModalProps) {
  const { t } = useTranslation();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const [query, setQuery] = useState("");
  const [isApplying, setIsApplying] = useState(false);
  const [selections, setSelections] = useState<TagSelections>({
    included: {},
    excluded: {}
  });

  const deferredQuery = useDeferredValue(query.trim());
  const { page, loading, loadingMore, resetPage, loadMore } = useTagListData({
    open,
    knownTags,
    query: deferredQuery,
    listRef,
    pageSize: PAGE_SIZE,
    loadMoreThresholdPx: LOAD_MORE_THRESHOLD_PX
  });
  const {
    clearPendingSingleClick,
    flushPendingSingleClick,
    scheduleSingleAction,
    handleDoubleAction
  } = useTagSelectionIntent({
    singleClickDelayMs: SINGLE_CLICK_DELAY_MS,
    onSelectionsChange: setSelections
  });
  const selectionSummary = useMemo(
    () =>
      t("tagList.selectionSummary", {
        included: Object.keys(selections.included).length,
        excluded: Object.keys(selections.excluded).length
      }),
    [selections.excluded, selections.included, t]
  );

  const resetState = () => {
    clearPendingSingleClick();
    setQuery("");
    setSelections({ included: {}, excluded: {} });
    setIsApplying(false);
    resetPage();
  };

  useEffect(() => {
    if (!open) {
      resetState();
      return;
    }

    const frameId = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [open, resetPage]);

  if (!open) {
    return null;
  }

  const handleTagClick = (tag: string, clickCount: number) => {
    if (clickCount > 1 || isApplying) {
      return;
    }

    scheduleSingleAction(tag);
  };

  const handleTagDoubleClick = (tag: string) => {
    if (isApplying) {
      return;
    }

    clearPendingSingleClick();
    handleDoubleAction(tag);
  };

  const handleApply = async () => {
    if (!onApplySearch || isApplying) {
      return;
    }

    const resolvedSelections = flushPendingSingleClick(selections);
    if (resolvedSelections !== selections) {
      setSelections(resolvedSelections);
    }

    setIsApplying(true);

    try {
      await onApplySearch(buildFilterInput(resolvedSelections));
      onClose();
    } catch {
      // keep the modal open so the user can retry
    } finally {
      setIsApplying(false);
    }
  };

  const handleListScroll = (event: UIEvent<HTMLDivElement>) => {
    const node = event.currentTarget;
    const remainingScroll = node.scrollHeight - node.scrollTop - node.clientHeight;
    if (remainingScroll <= LOAD_MORE_THRESHOLD_PX) {
      void loadMore();
    }
  };

  return (
    <UiModal
      open
      onClose={onClose}
      closeOnOverlayClick={!isApplying}
      closeOnEscape={!isApplying}
      size="medium"
      labelledBy="tag-list-modal-heading"
      contentClassName="flex max-h-[calc(100vh-2rem)] flex-col gap-3 overflow-hidden"
      testId="tag-list-modal"
    >
      <div className="flex items-start justify-between gap-3">
        <h2 id="tag-list-modal-heading" className="m-0 text-lg">
          {t("tagList.modalTitle")}
        </h2>

        <UiIconButton
          icon="close"
          onClick={onClose}
          disabled={isApplying}
          aria-label={t("tagList.closeModal")}
          title={t("tagList.closeModal")}
        />
      </div>

      <div className="grid gap-1.5">
        <label className="text-xs text-base-content/65" htmlFor="tag-list-filter-input">
          {t("tagList.filterLabel")}
        </label>
        <input
          id="tag-list-filter-input"
          ref={inputRef}
          value={query}
          className="w-full rounded-xl border border-base-content/12 bg-base-100 px-3 py-2.5 text-sm text-base-content shadow-[var(--shadow-surface)] transition-[box-shadow,border-color] duration-100 focus-visible:border-primary/40 focus-visible:outline-hidden focus-visible:shadow-[var(--field-shadow-focus)]"
          placeholder={t("tagList.filterPlaceholder")}
          aria-label={t("tagList.filterLabel")}
          disabled={isApplying}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
          {...browserAssistDisabledProps}
        />
      </div>

      <p className="m-0 text-xs text-base-content/65">{selectionSummary}</p>

      <div
        ref={listRef}
        className=" min-h-0 overflow-y-auto rounded-xl  p-2 "
        aria-label={t("tagList.listLabel")}
        onScroll={handleListScroll}
      >
        {loading && page.items.length === 0 ? (
          <p className="m-0 px-1 py-2 text-sm text-base-content/65">{t("tagList.loading")}</p>
        ) : page.items.length === 0 ? (
          <p className="m-0 px-1 py-2 text-sm text-base-content/65">
            {deferredQuery ? t("tagList.noMatches") : t("tagList.empty")}
          </p>
        ) : (
          <TagListChipList
            tags={page.items}
            selections={selections}
            disabled={isApplying}
            onTagClick={handleTagClick}
            onTagDoubleClick={handleTagDoubleClick}
            getStateLabel={(tag) => {
              const included = Boolean(selections.included[tag.trim().toLowerCase()]);
              const excluded = Boolean(selections.excluded[tag.trim().toLowerCase()]);
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

        {loadingMore ? (
          <p className="m-0 px-1 py-2 text-xs text-base-content/65">{t("tagList.loadingMore")}</p>
        ) : null}
      </div>

      <div className="flex items-center justify-end gap-2">
        <UiButton onClick={onClose} disabled={isApplying}>
          {t("common.cancel")}
        </UiButton>
        <UiButton onClick={handleApply} disabled={isApplying || !onApplySearch}>
          {isApplying ? t("tagList.applying") : t("common.apply")}
        </UiButton>
      </div>
    </UiModal>
  );
}
