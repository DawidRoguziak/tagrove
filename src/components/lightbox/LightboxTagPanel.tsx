import type { RefObject } from "react";
import { TagEditor } from "../UI/TagEditor";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
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
      className="grid min-h-0 min-w-0 shrink-0 grid-cols-[minmax(0,1fr)] content-start gap-2 border-t border-[var(--border-soft)] pt-4"
    >
      <h3 className="m-0 text-xs">{t("lightbox.taggingHeading")}</h3>

      <TagEditor
        variant="lightbox"
        tags={selectedTags}
        draft={tagDraft}
        knownTags={knownTags}
        inputRef={tagInputRef}
        inputId="lightbox-tag-draft-input"
        inputAriaLabel={t("lightbox.addTag")}
        placeholder={t("lightbox.tagInputPlaceholder")}
        listboxAriaLabel={t("lightbox.taggingSuggestions")}
        onDraftChange={onTagDraftChange}
        onAddTag={onAddTag}
        onRemoveTag={onRemoveTag}
        disabled={tagEditingDisabled}
        getRemoveTagAriaLabel={(tag) => t("lightbox.removeTagAria", { tag })}
      />

      {tagDetailsLoading ? (
        <p className="m-0 text-xs text-[var(--text-muted)]" role="status">{t("lightbox.tagDetailsLoading")}</p>
      ) : null}
      {tagDetailsFailed ? (
        <UiAlert tone="error" title={t("lightbox.tagDetailsLoadFailedTitle")}>
          <div className="grid gap-2">
            <span>{t("lightbox.tagDetailsLoadFailed")}</span>
            <UiButton className="btn-sm" onClick={onRetryTagDetails}>{t("lightbox.retryTagDetails")}</UiButton>
          </div>
        </UiAlert>
      ) : null}
      {tagSaving ? <p className="m-0 text-xs text-[var(--text-muted)]">{t("lightbox.tagSaving")}</p> : null}
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
