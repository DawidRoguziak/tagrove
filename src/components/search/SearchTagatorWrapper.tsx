import { useEffect, useMemo, useRef } from "react";
import { UiAlert } from "../UI/UiAlert";
import { UiIconButton } from "../UI/UiIconButton";
import { TagListSearchLauncher } from "../tag-list/TagListSearchLauncher";
import { SearchTagator } from "./SearchTagator";
import { useTranslation } from "react-i18next";
import type { SearchMediaKind } from "../app/types";
import type { SearchFilterValidationError } from "../../utils/media";

interface SearchTagatorWrapperProps {
  filterInput: string;
  onFilterChange: (value: string) => void;
  validationError?: SearchFilterValidationError | null;
  knownTags: string[];
  mediaKind: SearchMediaKind;
  onMediaKindChange: (value: SearchMediaKind) => void | Promise<void>;
  onSearchSubmit: () => void;
  onApplyTagListSearch?: (filterInput: string) => Promise<void> | void;
  onClearAll: () => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (value: boolean) => void | Promise<void>;
  submitOnParentCommit?: boolean;
}

export function SearchTagatorWrapper({
  filterInput,
  onFilterChange,
  validationError = null,
  knownTags,
  mediaKind,
  onMediaKindChange,
  onSearchSubmit,
  onApplyTagListSearch,
  onClearAll,
  favoritesOnly,
  onFavoritesOnlyChange,
  submitOnParentCommit = true
}: SearchTagatorWrapperProps) {
  const { t } = useTranslation();
  const submitAfterMediaKindChangeRef = useRef(false);
  const submitAfterFavoritesChangeRef = useRef(false);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const validationMessage = useMemo(() => {
    if (!validationError) {
      return null;
    }

    return t(`search.errors.${validationError}`);
  }, [t, validationError]);

  useEffect(() => {
    if (!submitOnParentCommit || !submitAfterMediaKindChangeRef.current) {
      return;
    }

    submitAfterMediaKindChangeRef.current = false;
    onSearchSubmit();
  }, [mediaKind, onSearchSubmit, submitOnParentCommit]);

  useEffect(() => {
    if (!submitOnParentCommit || !submitAfterFavoritesChangeRef.current) {
      return;
    }

    submitAfterFavoritesChangeRef.current = false;
    onSearchSubmit();
  }, [favoritesOnly, onSearchSubmit, submitOnParentCommit]);

  function handleClearAll() {
    onClearAll();
    requestAnimationFrame(() => {
      searchInputRef.current?.focus();
    });
  }

  return (
    <div className="grid w-full gap-2">
      <div className="grid w-full grid-cols-[44px_minmax(0,1fr)] gap-2 overflow-visible pr-12 lg:pr-0">
        <div className="grid place-items-center rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-1 shadow-[var(--shadow-control)] backdrop-blur-xl">
          <TagListSearchLauncher knownTags={knownTags} onApplySearch={onApplyTagListSearch} />
        </div>

        <div className="grid w-full grid-cols-[minmax(0,1fr)_148px] gap-1 overflow-visible rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-1 shadow-[var(--shadow-control)] backdrop-blur-xl transition-[border-color,box-shadow] duration-150 focus-within:border-primary/35 focus-within:shadow-[var(--field-shadow-focus)] sm:grid-cols-[minmax(0,1fr)_172px] lg:grid-cols-[minmax(0,1fr)_192px]">
          <SearchTagator
            inputRef={searchInputRef}
            value={filterInput}
            onValueChange={onFilterChange}
            knownTags={knownTags}
            onSubmit={onSearchSubmit}
            placeholder={t("search.placeholder")}
            listboxAriaLabel={t("search.tagSuggestions")}
            inputClassName="filter-input h-full w-full rounded-[10px] border-0 bg-transparent px-3 py-2.5 text-base-content shadow-none focus-visible:border-transparent focus-visible:outline-hidden focus-visible:shadow-none"
          />

          <div className="grid h-full grid-cols-[60px_44px_44px] overflow-hidden rounded-[9px] bg-base-200/72 sm:grid-cols-[84px_44px_44px] lg:grid-cols-[104px_44px_44px]">
            <select
              className="filter-kind-select rounded-none border-0 px-2 py-2.5 text-base-content shadow-none focus-visible:outline-hidden focus-visible:shadow-[0_0_0_2px_oklch(var(--p)/0.2)]"
              aria-label={t("search.mediaKind")}
              value={mediaKind}
              onChange={(event) => {
                submitAfterMediaKindChangeRef.current = submitOnParentCommit;
                void Promise.resolve(onMediaKindChange(event.target.value as SearchMediaKind)).catch(() => {});
              }}
            >
              <option value="all">{t("search.mediaKinds.all")}</option>
              <option value="image">{t("search.mediaKinds.image")}</option>
              <option value="gif">{t("search.mediaKinds.gif")}</option>
              <option value="video">{t("search.mediaKinds.video")}</option>
            </select>

            <UiIconButton
              icon="heart"
              active={favoritesOnly}
              className="h-full w-full min-h-0 rounded-none border-y-0 border-r-0 border-l border-base-content/10 bg-base-200/85 text-base-content/85 shadow-none hover:bg-base-200 hover:text-base-content focus-visible:shadow-[0_0_0_2px_oklch(var(--p)/0.2)]"
              onClick={() => {
                submitAfterFavoritesChangeRef.current = submitOnParentCommit;
                void Promise.resolve(onFavoritesOnlyChange(!favoritesOnly)).catch(() => {});
              }}
              aria-label={
                favoritesOnly ? t("search.favoritesOnly.disable") : t("search.favoritesOnly.enable")
              }
              title={
                favoritesOnly ? t("search.favoritesOnly.enabled") : t("search.favoritesOnly.label")
              }
            />

            <UiIconButton
              icon="reset"
              className="h-full w-full min-h-0 rounded-none border-y-0 border-r-0 border-l border-base-content/10 bg-base-200/85 text-base-content/85 shadow-none hover:bg-base-200 hover:text-base-content focus-visible:shadow-[0_0_0_2px_oklch(var(--p)/0.2)]"
              onClick={handleClearAll}
              aria-label={t("search.clearFilters")}
              title={t("search.clear")}
            />
          </div>
        </div>
      </div>

      {validationMessage ? <UiAlert tone="error">{validationMessage}</UiAlert> : null}
    </div>
  );
}
