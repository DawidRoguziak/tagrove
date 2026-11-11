import { onCleanup } from "solid-js";
import type { JSX } from "@solidjs/web";
import { UiIcon } from "../../ui/controls/UiIcon";
import { UiSegmentedControl } from "../../ui/controls/UiSegmentedControl";
import { UiButton } from "../../ui/controls/UiButton";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiIconButton } from "../../ui/controls/UiIconButton";
import { useLanguage } from "../../i18n/solid";
import { TagListSearchLauncher } from "./tag-list/TagListSearchLauncher";
import { SearchTagator } from "./SearchTagator";
import type { SearchFilterValidationError } from "../../utils/media";
import type { SuggestionClient } from "./suggestions/SuggestionClient";
type SearchMediaKind = "all" | "image" | "gif" | "video";
interface SearchTagatorWrapperProps {
  client: SuggestionClient;
  identity?: JSX.Element;
  headerAction?: JSX.Element;
  toolbarContent?: JSX.Element;
  filterInput: string;
  onFilterChange: (value: string) => void;
  validationError?: SearchFilterValidationError | null;
  knownTags: string[];
  mediaKind: SearchMediaKind;
  onMediaKindChange: (value: SearchMediaKind) => void | Promise<void>;
  onSearchSubmit: () => void;
  onApplyTagListSearch: (filterInput: string) => Promise<void>;
  onClearAll: () => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (value: boolean) => void | Promise<void>;
}
export function SearchTagatorWrapper(props: SearchTagatorWrapperProps) {
  const { t } = useLanguage();
  let input: HTMLInputElement | undefined;
  const focusSearch = (event: KeyboardEvent) => {
    if (
      !(event.ctrlKey || event.metaKey) ||
      event.altKey ||
      event.shiftKey ||
      event.key.toLowerCase() !== "k" ||
      event.isComposing
    )
      return;
    if (
      !input ||
      input.closest("[inert]") ||
      document.querySelector('[role="dialog"][aria-modal="true"]')
    )
      return;
    event.preventDefault();
    input.focus({ preventScroll: true });
  };
  window.addEventListener("keydown", focusSearch);
  let frame: number | null = null;
  onCleanup(() => {
    window.removeEventListener("keydown", focusSearch);
    if (frame !== null) cancelAnimationFrame(frame);
  });
  function handleClearAll() {
    props.onClearAll();
    if (frame !== null) cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => input?.focus());
  }
  const validationMessage = () =>
    props.validationError ? t(`search.errors.${props.validationError}`) : null;
  return (
    <>
      <div data-tauri-drag-region="deep" class="workspace-search window-drag-surface">
        {props.identity}
        <div
          data-tauri-drag-region="false"
          title={t("search.syntaxHint")}
          class="workspace-search-field flex min-w-0 items-center gap-2 rounded-[var(--radius-control)] border border-[var(--border-strong)] bg-[var(--surface-muted)] px-2 focus-within:border-primary"
        >
          <UiIcon name="search" className="h-4 w-4 shrink-0 text-[var(--text-muted)]" />
          <div class="min-w-0 flex-1">
            <SearchTagator
              client={props.client}
              inputRef={(node) => {
                input = node;
              }}
              ariaKeyShortcuts="Control+k Meta+k"
              value={props.filterInput}
              onValueChange={props.onFilterChange}
              knownTags={props.knownTags}
              onSubmit={props.onSearchSubmit}
              placeholder={t("search.placeholder")}
              listboxAriaLabel={t("search.tagSuggestions")}
              inputClassName="filter-input font-mono h-8 w-full border-0 bg-transparent px-1 py-2 text-base-content shadow-none focus-visible:shadow-none"
            />
          </div>
          <kbd class="search-shortcut" aria-hidden="true">
            {navigator.platform.includes("Mac") ? "⌘K" : "Ctrl K"}
          </kbd>
          <UiIconButton
            icon="reset"
            className="h-8! min-h-8! w-8! border-transparent! bg-transparent!"
            onClick={handleClearAll}
            aria-label={t("search.clearFilters")}
            title={t("search.clear")}
          />
          <UiButton
            variant="primary"
            className="h-7! min-h-7! text-xs"
            onClick={props.onSearchSubmit}
          >
            {t("workspace.search")}
          </UiButton>
        </div>
        {props.headerAction}
      </div>
      <div class="workspace-tools">
        <UiSegmentedControl
          id="gallery-media-kind"
          label={t("search.mediaKind")}
          options={(["all", "image", "gif", "video"] as const).map((kind) => ({
            value: kind,
            label: t(`search.mediaKinds.${kind}`)
          }))}
          value={props.mediaKind}
          onChange={(kind) => {
            void Promise.resolve(props.onMediaKindChange(kind)).catch(() => {});
          }}
        />
        <UiIconButton
          icon="heart"
          active={props.favoritesOnly}
          className="h-8! min-h-8! w-8!"
          onClick={() => {
            void Promise.resolve(props.onFavoritesOnlyChange(!props.favoritesOnly)).catch(() => {});
          }}
          aria-pressed={props.favoritesOnly ? "true" : "false"}
          aria-label={
            props.favoritesOnly
              ? t("search.favoritesOnly.disable")
              : t("search.favoritesOnly.enable")
          }
          title={
            props.favoritesOnly
              ? t("search.favoritesOnly.enabled")
              : t("search.favoritesOnly.label")
          }
        />
        <span class="mx-1 h-5 border-l border-[var(--border-strong)]" aria-hidden="true" />
        <TagListSearchLauncher
          className="h-8! min-h-8! w-8!"
          knownTags={props.knownTags}
          onApplySearch={props.onApplyTagListSearch}
        />
        {props.toolbarContent}
      </div>
      {validationMessage() ? (
        <div class="px-5 pb-2">
          <UiAlert tone="error">{validationMessage()}</UiAlert>
        </div>
      ) : null}
    </>
  );
}
