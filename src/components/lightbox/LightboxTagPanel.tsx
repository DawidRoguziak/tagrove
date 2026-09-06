import type { RefObject } from "react";
import { AssignedTagList } from "../UI/AssignedTagList";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
import { SearchTagator } from "../search/SearchTagator";
import { useTranslation } from "react-i18next";

interface LightboxTagPanelProps {
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: RefObject<HTMLInputElement | null>;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagEditingDisabled: boolean;
  onRemoveTag: (tag: string) => void;
  onTagDraftChange: (value: string) => void;
  onAddTag: (value: string) => void;
  onRetryTags: () => void;
  onRetryTagDetails: () => void;
}

export function LightboxTagPanel({
  selectedTags,
  tagDraft,
  knownTags,
  tagInputRef,
  tagSaving,
  tagFailed,
  tagDetailsLoading,
  tagDetailsFailed,
  tagEditingDisabled,
  onRemoveTag,
  onTagDraftChange,
  onAddTag,
  onRetryTags,
  onRetryTagDetails
}: LightboxTagPanelProps) {
  const { t } = useTranslation();

  return (
    <section
      aria-label={t("lightbox.taggingHeading")}
      data-testid="lightbox-tag-panel"
      className="grid min-h-0 min-w-0 shrink-0 grid-cols-[minmax(0,1fr)] content-start gap-2 border-t border-[var(--border-soft)] pt-2"
    >
      <h3 className="m-0 text-xs">{t("lightbox.taggingHeading")}</h3>

      <AssignedTagList
        variant="lightbox"
        tags={selectedTags}
        emptyText={t("lightbox.noTags")}
        onRemoveTag={onRemoveTag}
        removeDisabled={tagEditingDisabled}
        getRemoveTagAriaLabel={(tag) => t("lightbox.removeTagAria", { tag })}
      />

      <div className="relative grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1">
        <label className="text-[11px] text-base-content/65" htmlFor="lightbox-tag-draft-input">
          {t("lightbox.addTag")}
        </label>
        <SearchTagator
          inputId="lightbox-tag-draft-input"
          inputRef={tagInputRef}
          value={tagDraft}
          onValueChange={onTagDraftChange}
          knownTags={knownTags}
          excludedTags={selectedTags}
          onSuggestionPick={onAddTag}
          onSubmit={() => onAddTag(tagDraft)}
          placeholder={t("lightbox.tagInputPlaceholder")}
          listboxAriaLabel={t("lightbox.taggingSuggestions")}
          inputClassName="h-8 w-full"
          keepSuggestionsOpenOnPick
          autoSelectFirstSuggestion={false}
          suggestionsPlacement="below"
          suggestionsStrategy="viewport"
          disabled={tagEditingDisabled}
        />
      </div>

      {tagDetailsLoading ? (
        <p className="m-0 text-[11px] text-base-content/60" role="status">{t("lightbox.tagDetailsLoading")}</p>
      ) : null}
      {tagDetailsFailed ? (
        <UiAlert tone="error" title={t("lightbox.tagDetailsLoadFailedTitle")}>
          <div className="grid gap-2">
            <span>{t("lightbox.tagDetailsLoadFailed")}</span>
            <UiButton className="btn-sm" onClick={onRetryTagDetails}>{t("lightbox.retryTagDetails")}</UiButton>
          </div>
        </UiAlert>
      ) : null}
      {tagSaving ? <p className="m-0 text-[11px] text-base-content/60">{t("lightbox.tagSaving")}</p> : null}
      {tagFailed ? (
        <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
          <div className="grid gap-2">
            <span>{t("bulk.panel.tagSaveFailed")}</span>
            <UiButton className="btn-sm" onClick={onRetryTags}>{t("lightbox.retryTagSave")}</UiButton>
          </div>
        </UiAlert>
      ) : null}

    </section>
  );
}
