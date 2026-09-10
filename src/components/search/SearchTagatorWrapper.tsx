import { useEffect, useId, useMemo, useRef, type ReactNode } from "react";
import { UiIcon } from "../UI/UiIcon";
import { UiButton } from "../UI/UiButton";
import { UiAlert } from "../UI/UiAlert";
import { UiIconButton } from "../UI/UiIconButton";
import { TagListSearchLauncher } from "../tag-list/TagListSearchLauncher";
import { SearchTagator } from "./SearchTagator";
import { useTranslation } from "react-i18next";
import type { SearchMediaKind } from "../app/types";
import type { SearchFilterValidationError } from "../../utils/media";

interface SearchTagatorWrapperProps {
  identity?: ReactNode;
  headerAction?: ReactNode;
  toolbarContent?: ReactNode;
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
  identity,
  headerAction,
  toolbarContent,
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
  const mediaKindName = useId();
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
    <>
      <div data-tauri-drag-region="deep" className="workspace-search window-drag-surface">
        {identity}
        <div data-tauri-drag-region="false" className="workspace-search-field flex min-w-0 items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-strong)] bg-[var(--surface-muted)] px-2 focus-within:border-primary">
          <UiIcon name="search" className="h-4 w-4 shrink-0 text-[var(--text-muted)]" />
          <div className="min-w-0 flex-1"><SearchTagator
            inputRef={searchInputRef}
            value={filterInput}
            onValueChange={onFilterChange}
            knownTags={knownTags}
            onSubmit={onSearchSubmit}
            placeholder={t("search.placeholder")}
            listboxAriaLabel={t("search.tagSuggestions")}
            inputClassName="filter-input h-9 w-full border-0 bg-transparent px-1 py-2 text-base-content shadow-none focus-visible:shadow-none"
          /></div>
          <UiIconButton icon="reset" className="h-8! min-h-8! w-8! border-transparent! bg-transparent!"
            onClick={handleClearAll} aria-label={t("search.clearFilters")} title={t("search.clear")} />
          <UiButton variant="primary" className="h-7! min-h-7! text-xs" onClick={onSearchSubmit}>
            {t("workspace.search")}
          </UiButton>
        </div>
        {headerAction}
      </div>
      <div className="workspace-tools">
        <fieldset className="media-segments" id="gallery-media-kind">
          <legend className="sr-only">{t("search.mediaKind")}</legend>
          {(["all", "image", "gif", "video"] as const).map((kind) => (
            <label key={kind}>
              <input
                className="choice-input"
                type="radio"
                name={mediaKindName}
                value={kind}
                checked={mediaKind === kind}
                onChange={() => {
                  submitAfterMediaKindChangeRef.current = submitOnParentCommit;
                  void Promise.resolve(onMediaKindChange(kind)).catch(() => {});
                }}
              />
              <span>{t(`search.mediaKinds.${kind}`)}</span>
            </label>
          ))}
        </fieldset>
        <UiIconButton icon="heart" active={favoritesOnly} className="h-8! min-h-8! w-8!"
          onClick={() => {
            submitAfterFavoritesChangeRef.current = submitOnParentCommit;
            void Promise.resolve(onFavoritesOnlyChange(!favoritesOnly)).catch(() => {});
          }}
          aria-pressed={favoritesOnly}
          aria-label={favoritesOnly ? t("search.favoritesOnly.disable") : t("search.favoritesOnly.enable")}
          title={favoritesOnly ? t("search.favoritesOnly.enabled") : t("search.favoritesOnly.label")}
        />
        <span className="mx-1 h-5 border-l border-[var(--border-strong)]" aria-hidden="true" />
        <TagListSearchLauncher className="h-8! min-h-8! w-8!" knownTags={knownTags} onApplySearch={onApplyTagListSearch} />
        {toolbarContent}
      </div>
      {validationMessage ? <div className="px-5 pb-2"><UiAlert tone="error">{validationMessage}</UiAlert></div> : null}
    </>
  );
}
