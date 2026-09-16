import { useCallback, useLayoutEffect, useRef, type RefObject } from "react";
import { SearchTagator } from "../search/SearchTagator";
import { UiIcon } from "./UiIcon";

interface TagEditorProps {
  variant: "lightbox" | "bulk";
  tags: string[];
  emptyText?: string;
  draft: string;
  knownTags: string[];
  inputRef: RefObject<HTMLInputElement | null>;
  inputId: string;
  inputAriaLabel: string;
  placeholder: string;
  listboxAriaLabel: string;
  onDraftChange: (value: string) => void;
  onAddTag: (tag: string) => void;
  disabled?: boolean;
  loadingText?: string;
  onRemoveTag?: (tag: string) => void;
  getRemoveTagAriaLabel?: (tag: string) => string;
}

export function TagEditor({
  variant,
  tags,
  emptyText,
  loadingText,
  onRemoveTag,
  getRemoveTagAriaLabel,
  draft, knownTags, inputRef, inputId, inputAriaLabel, placeholder,
  listboxAriaLabel, onDraftChange, onAddTag, disabled = false
}: TagEditorProps) {
  const fieldRef = useRef<HTMLDivElement>(null);
  const editingDisabled = disabled || loadingText !== undefined;
  const revealInput = useCallback(() => {
    const field = fieldRef.current;
    const input = inputRef.current;
    if (!field || !input || document.activeElement !== input) return;
    const fieldBounds = field.getBoundingClientRect();
    const inputBounds = input.getBoundingClientRect();
    if (inputBounds.bottom > fieldBounds.bottom) field.scrollTop += inputBounds.bottom - fieldBounds.bottom;
    else if (inputBounds.top < fieldBounds.top) field.scrollTop -= fieldBounds.top - inputBounds.top;
  }, [inputRef]);
  // Keep only the field scrolled to the caret when chips wrap after an edit or resize.
  useLayoutEffect(() => {
    revealInput();
  });
  useLayoutEffect(() => {
    const observer = new ResizeObserver(revealInput);
    if (fieldRef.current) observer.observe(fieldRef.current);
    if (inputRef.current) observer.observe(inputRef.current);
    return () => observer.disconnect();
  }, [inputRef, revealInput]);

  const isLightbox = variant === "lightbox";
  const containerClassName = isLightbox
    ? "panel-scroll flex max-h-[200px] min-h-[48px] flex-wrap items-start gap-1.5 overflow-auto rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-base-200/38 p-2"
    : "panel-scroll flex min-h-0 max-h-36 flex-wrap items-start gap-1.5 overflow-y-auto overscroll-contain rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-2";
  const removeButtonClassName = isLightbox
    ? "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-[var(--radius-control)] bg-transparent text-primary-text hover:bg-error hover:text-error-content focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary/40"
    : "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-[var(--radius-control)] bg-transparent text-primary-text hover:bg-error hover:text-error-content";
  const statusClassName = isLightbox
    ? "px-1 text-xs text-[var(--text-muted)]"
    : "text-xs text-[var(--text-muted)]";

  return (
    <div
      ref={fieldRef}
      className={`${containerClassName} tag-editor`}
      onFocusCapture={revealInput}
      onClick={(event) => {
        if (event.target === event.currentTarget && !editingDisabled) inputRef.current?.focus();
      }}
      data-testid={isLightbox ? "lightbox-tag-list" : "bulk-tag-list"}
      data-ui="tag-editor"
      aria-busy={loadingText !== undefined}
    >
      {loadingText !== undefined ? (
        <span className={statusClassName}>{loadingText}</span>
      ) : tags.length > 0 ? (
        tags.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1.5 rounded-[var(--radius-control)] max-w-full border border-transparent bg-[var(--surface-raised)] px-2 py-1 font-mono text-xs font-medium leading-4 text-primary-text shadow-[var(--shadow-chip)]"
          >
            <span className="min-w-0 break-all">{tag}</span>
            {onRemoveTag && getRemoveTagAriaLabel ? (
              <button
                type="button"
                className={removeButtonClassName}
                aria-label={getRemoveTagAriaLabel(tag)}
                disabled={editingDisabled}
                onClick={() => onRemoveTag(tag)}
              >
                <UiIcon name="close" className="h-3 w-3 [stroke-width:2.4]" />
              </button>
            ) : null}
          </span>
        ))
      ) : emptyText ? (
        <span className={statusClassName}>{emptyText}</span>
      ) : null}
      <SearchTagator
        inputId={inputId}
        inputRef={inputRef}
        value={draft}
        onValueChange={onDraftChange}
        knownTags={knownTags}
        excludedTags={tags}
        onSuggestionPick={onAddTag}
        onSubmit={() => onAddTag(draft)}
        ariaLabel={inputAriaLabel}
        placeholder={placeholder}
        listboxAriaLabel={listboxAriaLabel}
        wrapperClassName="flex-[1_1_9rem] max-w-full"
        inputClassName="h-8 w-full min-w-0 border-0 bg-transparent px-1 shadow-none outline-none focus-visible:shadow-none focus-visible:outline-none"
        keepSuggestionsOpenOnPick
        autoSelectFirstSuggestion={false}
        suggestionsStrategy="viewport"
        disabled={editingDisabled}
      />
    </div>
  );
}
