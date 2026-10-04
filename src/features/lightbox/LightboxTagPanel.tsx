import type { SuggestionClient } from "../search/suggestions/SuggestionClient";
import { TagEditor } from "../tagging/TagEditor";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiButton } from "../../ui/controls/UiButton";
import { useLanguage } from "../../i18n/solid";

interface LightboxTagPanelProps {
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: (node: HTMLInputElement) => void;
  client: SuggestionClient;
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

export function LightboxTagPanel(props: LightboxTagPanelProps) {
  const { t } = useLanguage();

  return (
    <section
      aria-label={t("lightbox.taggingHeading")}
      data-testid="lightbox-tag-panel"
      class="grid min-h-0 min-w-0 shrink-0 grid-cols-[minmax(0,1fr)] content-start gap-2 border-t border-[var(--border-soft)] pt-4"
    >
      <h3 class="m-0 text-xs">{t("lightbox.taggingHeading")}</h3>

      <TagEditor
        variant="lightbox"
        tags={props.selectedTags}
        draft={props.tagDraft}
        client={props.client}
        knownTags={props.knownTags}
        inputRef={props.tagInputRef}
        inputId="lightbox-tag-draft-input"
        inputAriaLabel={t("lightbox.addTag")}
        placeholder={t("lightbox.tagInputPlaceholder")}
        listboxAriaLabel={t("lightbox.taggingSuggestions")}
        onDraftChange={props.onTagDraftChange}
        onAddTag={props.onAddTag}
        onRemoveTag={props.onRemoveTag}
        disabled={props.tagEditingDisabled}
        getRemoveTagAriaLabel={(tag) => t("lightbox.removeTagAria", { tag })}
      />

      {props.tagDetailsLoading ? (
        <p class="m-0 text-xs text-[var(--text-muted)]" role="status">
          {t("lightbox.tagDetailsLoading")}
        </p>
      ) : null}
      {props.tagDetailsFailed ? (
        <UiAlert tone="error" title={t("lightbox.tagDetailsLoadFailedTitle")}>
          <div class="grid gap-2">
            <span>{t("lightbox.tagDetailsLoadFailed")}</span>
            <UiButton className="btn-sm" onClick={props.onRetryTagDetails}>
              {t("lightbox.retryTagDetails")}
            </UiButton>
          </div>
        </UiAlert>
      ) : null}
      {props.tagSaving ? (
        <p class="m-0 text-xs text-[var(--text-muted)]">{t("lightbox.tagSaving")}</p>
      ) : null}
      {props.tagFailed ? (
        <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
          <div class="grid gap-2">
            <span>{t("bulk.panel.tagSaveFailed")}</span>
            <UiButton className="btn-sm" onClick={props.onRetryTags}>
              {t("lightbox.retryTagSave")}
            </UiButton>
          </div>
        </UiAlert>
      ) : null}
    </section>
  );
}
