import { UiIconButton } from "../UI/UiIconButton";
import { SearchTagatorWrapper } from "../search/SearchTagatorWrapper";
import { useTranslation } from "react-i18next";
import type { SearchFilterValidationError } from "../../utils/media";

interface TopBarProps {
  filterInput: string;
  onFilterChange: (value: string) => void;
  filterValidationError?: SearchFilterValidationError | null;
  knownTags: string[];
  mediaKind: "all" | "image" | "gif" | "video";
  onMediaKindChange: (value: "all" | "image" | "gif" | "video") => void;
  onSearchSubmit: () => void;
  onApplyTagListSearch: (filterInput: string) => Promise<void> | void;
  onClearSearch: () => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (value: boolean) => void;
  onOpenSettingsView: () => void;
}

export function TopBar({
  filterInput,
  onFilterChange,
  filterValidationError = null,
  knownTags,
  mediaKind,
  onMediaKindChange,
  onSearchSubmit,
  onApplyTagListSearch,
  onClearSearch,
  favoritesOnly,
  onFavoritesOnlyChange,
  onOpenSettingsView
}: TopBarProps) {
  const { t } = useTranslation();

  return (
    <section className="sticky top-0 z-40 overflow-visible border-b border-[var(--border-soft)] bg-[var(--window-chrome-bg)] px-3 py-2.5 shadow-[var(--shadow-topbar)] backdrop-blur-xl sm:px-5">
      <div className="relative mx-auto grid w-full max-w-[1480px] min-h-0 grid-cols-1 items-center gap-2.5 lg:flex lg:min-h-11 lg:justify-center">
        <div className="grid w-full justify-items-stretch gap-2.5 lg:w-[min(880px,calc(100%-190px))] lg:justify-items-center">
          <SearchTagatorWrapper
            filterInput={filterInput}
            onFilterChange={onFilterChange}
            validationError={filterValidationError}
            knownTags={knownTags}
            mediaKind={mediaKind}
            onMediaKindChange={onMediaKindChange}
            onSearchSubmit={onSearchSubmit}
            onApplyTagListSearch={onApplyTagListSearch}
            onClearAll={onClearSearch}
            favoritesOnly={favoritesOnly}
            onFavoritesOnlyChange={onFavoritesOnlyChange}
            submitOnParentCommit={false}
          />
        </div>

        <div className="absolute right-0 top-1/2 flex -translate-y-1/2 items-center">
          <UiIconButton
            id="open-settings-button"
            icon="settings"
            onClick={onOpenSettingsView}
            aria-label={t("topBar.openSettings")}
            title={t("topBar.openSettings")}
          />
        </div>
      </div>
    </section>
  );
}
