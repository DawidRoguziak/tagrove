import { createEffect, onSettled, For } from "solid-js";
import type { SuggestionClient } from "../search/suggestions/SuggestionClient";
import { SearchTagator } from "../search/SearchTagator";
import { UiIcon } from "../../ui/controls/UiIcon";

interface TagEditorProps {
  variant: "lightbox" | "bulk";
  tags: string[];
  emptyText?: string;
  draft: string;
  knownTags: string[];
  inputRef: (node: HTMLInputElement) => void;
  client: SuggestionClient;
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

export function TagEditor(props: TagEditorProps) {
  let field: HTMLDivElement | undefined;
  let input: HTMLInputElement | undefined;
  const editingDisabled = () => props.disabled || props.loadingText !== undefined;
  const revealInput = () => {
    if (!field || !input || document.activeElement !== input) return;
    const fieldBounds = field.getBoundingClientRect();
    const inputBounds = input.getBoundingClientRect();
    if (inputBounds.bottom > fieldBounds.bottom)
      field.scrollTop += inputBounds.bottom - fieldBounds.bottom;
    else if (inputBounds.top < fieldBounds.top)
      field.scrollTop -= fieldBounds.top - inputBounds.top;
  };
  createEffect(() => [props.tags, props.draft, props.disabled], revealInput);
  onSettled(() => {
    const observer = new ResizeObserver(revealInput);
    if (field) observer.observe(field);
    if (input) observer.observe(input);
    return () => observer.disconnect();
  });
  const isLightbox = () => props.variant === "lightbox";
  const containerClassName = () =>
    isLightbox()
      ? "panel-scroll flex max-h-[200px] min-h-[48px] flex-wrap items-start gap-1.5 overflow-auto rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-base-200/38 p-2"
      : "panel-scroll flex min-h-0 max-h-36 flex-wrap items-start gap-1.5 overflow-y-auto overscroll-contain rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-2";
  const removeButtonClassName = () =>
    isLightbox()
      ? "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-[var(--radius-control)] bg-transparent text-primary-text hover:bg-error hover:text-error-content focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary/40"
      : "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-[var(--radius-control)] bg-transparent text-primary-text hover:bg-error hover:text-error-content";
  const statusClassName = () =>
    isLightbox() ? "px-1 text-xs text-[var(--text-muted)]" : "text-xs text-[var(--text-muted)]";

  return (
    <div
      ref={(node) => {
        field = node;
      }}
      class={`${containerClassName()} tag-editor`}
      onFocusIn={revealInput}
      onClick={(event) => {
        if (event.target === event.currentTarget && !editingDisabled()) input?.focus();
      }}
      data-testid={isLightbox() ? "lightbox-tag-list" : "bulk-tag-list"}
      data-ui="tag-editor"
      aria-busy={props.loadingText !== undefined ? "true" : "false"}
    >
      {props.loadingText !== undefined ? (
        <span class={statusClassName()}>{props.loadingText}</span>
      ) : props.tags.length > 0 ? (
        <For each={props.tags}>
          {(tag) => (
            <span class="inline-flex items-center gap-1.5 rounded-[var(--radius-control)] max-w-full border border-transparent bg-[var(--surface-raised)] px-2 py-1 font-mono text-xs font-medium leading-4 text-primary-text shadow-[var(--shadow-chip)]">
              <span class="min-w-0 break-all">{tag}</span>
              {props.onRemoveTag && props.getRemoveTagAriaLabel ? (
                <button
                  type="button"
                  class={removeButtonClassName()}
                  aria-label={props.getRemoveTagAriaLabel?.(tag)}
                  disabled={editingDisabled()}
                  onClick={() => props.onRemoveTag?.(tag)}
                >
                  <UiIcon name="close" className="h-3 w-3 [stroke-width:2.4]" />
                </button>
              ) : null}
            </span>
          )}
        </For>
      ) : props.emptyText ? (
        <span class={statusClassName()}>{props.emptyText}</span>
      ) : null}
      <SearchTagator
        inputId={props.inputId}
        inputRef={(node) => {
          input = node;
          props.inputRef(node);
        }}
        client={props.client}
        value={props.draft}
        onValueChange={props.onDraftChange}
        knownTags={props.knownTags}
        excludedTags={props.tags}
        onSuggestionPick={props.onAddTag}
        onSubmit={() => props.onAddTag(props.draft)}
        ariaLabel={props.inputAriaLabel}
        placeholder={props.placeholder}
        listboxAriaLabel={props.listboxAriaLabel}
        wrapperClassName="flex-[1_1_9rem] max-w-full"
        inputClassName="h-8 w-full min-w-0 border-0 bg-transparent px-1 shadow-none outline-none focus-visible:shadow-none focus-visible:outline-none"
        keepSuggestionsOpenOnPick
        autoSelectFirstSuggestion={false}
        suggestionsStrategy="viewport"
        disabled={editingDisabled()}
      />
    </div>
  );
}
