import { WindowControls } from "../app/WindowControls";
import type { ReactNode } from "react";
import { UiIcon } from "../UI/UiIcon";
import { UiIconButton } from "../UI/UiIconButton";
import { SearchTagatorWrapper } from "../search/SearchTagatorWrapper";
import { useTranslation } from "react-i18next";
import type { SearchFilterValidationError } from "../../utils/media";

interface TopBarProps {
  toolbarContent?: ReactNode;
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
  toolbarContent,
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
    <header className="workspace-header">
      <SearchTagatorWrapper
        identity={<div className="workspace-identity"><UiIcon name="group" className="h-6 w-6 text-primary" /><span>MediaTagger</span></div>}
        headerAction={<div className="flex items-center gap-3"><UiIconButton id="open-settings-button" icon="settings" onClick={onOpenSettingsView}
          aria-label={t("topBar.openSettings")} title={t("topBar.openSettings")} /><WindowControls /></div>}
        toolbarContent={toolbarContent}
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
    </header>
  );
}
