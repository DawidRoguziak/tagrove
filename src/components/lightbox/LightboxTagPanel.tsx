import type { RefObject } from "react";
import { AssignedTagList } from "../UI/AssignedTagList";
import { UiAlert } from "../UI/UiAlert";
import { UiButton } from "../UI/UiButton";
import { UiIconButton } from "../UI/UiIconButton";
import { SearchTagator } from "../search/SearchTagator";
import { MediaGroupSetter } from "./MediaGroupSetter";
import { useTranslation } from "react-i18next";

interface LightboxTagPanelProps {
  open: boolean;
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: RefObject<HTMLInputElement | null>;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagEditingDisabled: boolean;
  mediaGroupKey: string;
  mediaGroupOrder: string;
  onToggle: () => void;
  onRemoveTag: (tag: string) => void;
  onTagDraftChange: (value: string) => void;
  onAddTag: (value: string) => void;
  onRetryTags: () => void;
  onRetryTagDetails: () => void;
  onMediaGroupKeyChange: (value: string) => void;
  onMediaGroupOrderChange: (value: string) => void;
  onApplyMediaGroup: () => void;
}

export function LightboxTagPanel({
  open,
  selectedTags,
  tagDraft,
  knownTags,
  tagInputRef,
  tagSaving,
  tagFailed,
  tagDetailsLoading,
  tagDetailsFailed,
  tagEditingDisabled,
  mediaGroupKey,
  mediaGroupOrder,
  onToggle,
  onRemoveTag,
  onTagDraftChange,
  onAddTag,
  onRetryTags,
  onRetryTagDetails,
  onMediaGroupKeyChange,
  onMediaGroupOrderChange,
  onApplyMediaGroup
}: LightboxTagPanelProps) {
  const { t } = useTranslation();

  return (
    <div className="relative">
      <UiIconButton
        icon="tag"
        active={open}
        aria-expanded={open}
        aria-controls="lightbox-tag-panel"
        aria-label={t("lightbox.showTagging")}
        title={t("lightbox.tags")}
        onClick={onToggle}
      />
      <aside
        id="lightbox-tag-panel"
        aria-hidden={!open}
        ref={(node) => node?.toggleAttribute("inert", !open)}
        className={`panel-scroll absolute z-[6] grid w-[min(380px,calc(100vw-2.5rem))] content-start gap-4 rounded-[var(--radius-surface)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-4 shadow-[var(--shadow-popover)] backdrop-blur-xl transition-[opacity,translate] duration-150 bottom-[calc(100%+10px)] right-0 lg:bottom-auto lg:right-[calc(100%+12px)] lg:top-1/2 lg:-translate-y-1/2 ${
          open
            ? "pointer-events-auto translate-y-0 opacity-100 lg:translate-y-[-50%]"
            : "pointer-events-none translate-y-2 opacity-0 lg:translate-y-[calc(-50%-6px)]"
        }`}
      >
        <h2 className="m-0 text-base">{t("lightbox.taggingHeading")}</h2>

        <AssignedTagList
          variant="lightbox"
          tags={selectedTags}
          emptyText={t("lightbox.noTags")}
          onRemoveTag={onRemoveTag}
          removeDisabled={tagEditingDisabled}
          getRemoveTagAriaLabel={(tag) => t("lightbox.removeTagAria", { tag })}
        />

        <div className="relative grid gap-1.5">
          <label className="text-xs text-base-content/65" htmlFor="lightbox-tag-draft-input">
            {t("lightbox.addTag")}
          </label>
          <SearchTagator
            inputId="lightbox-tag-draft-input"
            inputRef={tagInputRef}
            value={tagDraft}
            onValueChange={onTagDraftChange}
            knownTags={knownTags}
            onSuggestionPick={onAddTag}
            onSubmit={() => onAddTag(tagDraft)}
            placeholder={t("lightbox.tagInputPlaceholder")}
            listboxAriaLabel={t("lightbox.taggingSuggestions")}
            inputClassName="w-full"
            keepSuggestionsOpenOnPick
            autoSelectFirstSuggestion={false}
            disabled={tagEditingDisabled}
          />
        </div>

        {tagDetailsLoading ? (
          <p className="m-0 text-xs text-base-content/60" role="status">{t("lightbox.tagDetailsLoading")}</p>
        ) : null}
        {tagDetailsFailed ? (
          <UiAlert tone="error" title={t("lightbox.tagDetailsLoadFailedTitle")}>
            <div className="grid gap-2">
              <span>{t("lightbox.tagDetailsLoadFailed")}</span>
              <UiButton className="btn-sm" onClick={onRetryTagDetails}>{t("lightbox.retryTagDetails")}</UiButton>
            </div>
          </UiAlert>
        ) : null}
        {tagSaving ? <p className="m-0 text-xs text-base-content/60">{t("lightbox.tagSaving")}</p> : null}
        {tagFailed ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            <div className="grid gap-2">
              <span>{t("bulk.panel.tagSaveFailed")}</span>
              <UiButton className="btn-sm" onClick={onRetryTags}>{t("lightbox.retryTagSave")}</UiButton>
            </div>
          </UiAlert>
        ) : null}

        <div className="grid gap-1.5">
          <label className="text-xs text-base-content/65" htmlFor="lightbox-media-group-key-input">
            {t("lightbox.mediaGroup")}
          </label>
          <MediaGroupSetter
            groupKey={mediaGroupKey}
            groupOrder={mediaGroupOrder}
            onGroupKeyChange={onMediaGroupKeyChange}
            onGroupOrderChange={onMediaGroupOrderChange}
            onApply={onApplyMediaGroup}
          />
        </div>
      </aside>
    </div>
  );
}
