import { getTagMode, type TagSelections } from "../services/tagListSelectionService";

interface TagListChipListProps {
  tags: string[];
  selections: TagSelections;
  disabled: boolean;
  onTagClick: (tag: string, clickCount: number) => void;
  onTagDoubleClick: (tag: string) => void;
  getStateLabel: (tag: string) => string;
  getButtonTitle: (tag: string, state: string) => string;
  getButtonAriaLabel: (tag: string, state: string) => string;
}

export function TagListChipList({
  tags,
  selections,
  disabled,
  onTagClick,
  onTagDoubleClick,
  getStateLabel,
  getButtonTitle,
  getButtonAriaLabel
}: TagListChipListProps) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {tags.map((tag) => {
        const tagMode = getTagMode(selections, tag);
        const stateLabel = getStateLabel(tag);
        const tagClasses = [
          "inline-flex break-all items-center rounded-full border px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary/35",
          tagMode === "include"
            ? "border-accent bg-accent text-accent-content hover:border-accent hover:bg-accent/90"
            : tagMode === "exclude"
              ? "border-error/45 bg-error/12 text-error hover:border-error/60 hover:bg-error/18"
              : "border-base-content/12 bg-base-100 text-base-content hover:border-base-content/24 hover:bg-base-100/80"
        ]
          .filter(Boolean)
          .join(" ");

        return (
          <button
            key={tag}
            type="button"
            className={tagClasses}
            aria-pressed={tagMode !== null}
            aria-label={getButtonAriaLabel(tag, stateLabel)}
            title={getButtonTitle(tag, stateLabel)}
            disabled={disabled}
            onClick={(event) => onTagClick(tag, event.detail)}
            onDoubleClick={() => onTagDoubleClick(tag)}
          >
            {tag}
          </button>
        );
      })}
    </div>
  );
}
