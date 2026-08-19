import { UiIcon } from "./UiIcon";

interface AssignedTagListProps {
  variant: "lightbox" | "bulk";
  tags: string[];
  emptyText: string;
  loadingText?: string;
  onRemoveTag?: (tag: string) => void;
  getRemoveTagAriaLabel?: (tag: string) => string;
  removeDisabled?: boolean;
}

export function AssignedTagList({
  variant,
  tags,
  emptyText,
  loadingText,
  onRemoveTag,
  getRemoveTagAriaLabel,
  removeDisabled = false
}: AssignedTagListProps) {
  const isLightbox = variant === "lightbox";
  const containerClassName = isLightbox
    ? "panel-scroll flex max-h-[200px] min-h-[48px] flex-wrap items-start gap-1.5 overflow-auto rounded-xl border border-[var(--border-soft)] bg-base-200/38 px-0 py-2"
    : "panel-scroll flex min-h-0 max-h-36 flex-wrap items-start gap-1.5 overflow-y-auto overscroll-contain rounded-lg border border-base-content/12 px-0 py-2";
  const removeButtonClassName = isLightbox
    ? "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-content text-accent transition-colors hover:bg-error hover:text-error-content focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary/40"
    : "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-content text-accent hover:bg-error hover:text-error-content";
  const statusClassName = isLightbox
    ? "px-1 text-[11px] text-base-content/60"
    : "text-xs text-base-content/60";

  return (
    <div
      className={containerClassName}
      data-testid={isLightbox ? "lightbox-tag-list" : "bulk-tag-list"}
      data-ui="assigned-tag-list"
    >
      {loadingText !== undefined ? (
        <span className={statusClassName}>{loadingText}</span>
      ) : tags.length > 0 ? (
        tags.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1.5 rounded-full bg-accent px-2.5 py-1.5 text-xs font-semibold leading-none text-accent-content shadow-[var(--shadow-chip)]"
          >
            <span>{tag}</span>
            {onRemoveTag && getRemoveTagAriaLabel ? (
              <button
                type="button"
                className={removeButtonClassName}
                aria-label={getRemoveTagAriaLabel(tag)}
                disabled={removeDisabled}
                onClick={() => onRemoveTag(tag)}
              >
                <UiIcon name="close" className="h-3 w-3 [stroke-width:2.4]" />
              </button>
            ) : null}
          </span>
        ))
      ) : (
        <span className={statusClassName}>{emptyText}</span>
      )}
    </div>
  );
}
