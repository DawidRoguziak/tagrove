import {
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  onCleanup,
  untrack
} from "solid-js";
import { useLanguage } from "../../i18n/solid";
import type { SuggestionClient } from "../../components/search/worker/SuggestionClient";
import { collectUsedTags, readActiveToken } from "../../components/search/services/tokenService";
import { applySuggestionToValue } from "../../components/search/services/inputService";
import { createSuggestions } from "./suggestions/createSuggestions";
import { SearchSuggestionsList } from "./suggestions/SearchSuggestionsList";
const EMPTY: string[] = [];
export function SearchTagator(props: {
  client: SuggestionClient;
  value: string;
  knownTags: string[];
  onValueChange: (value: string) => void;
  onSubmit?: () => void;
  onSuggestionPick?: (value: string) => void;
  excludedTags?: string[];
  placeholder?: string;
  ariaLabel?: string;
  ariaKeyShortcuts?: string;
  listboxAriaLabel?: string;
  inputId?: string;
  inputRef?: (node: HTMLInputElement) => void;
  inputClassName?: string;
  wrapperClassName?: string;
  keepSuggestionsOpenOnPick?: boolean;
  autoSelectFirstSuggestion?: boolean;
  suggestionsPlacement?: "above" | "below";
  suggestionsStrategy?: "inline" | "viewport";
  disabled?: boolean;
}) {
  const { t } = useLanguage();
  const [caret, setCaret] = createSignal(0);
  const [active, setActive] = createSignal(0);
  const [open, setOpen] = createSignal(false);
  const [focused, setFocused] = createSignal(false);
  const [input, setInput] = createSignal<HTMLInputElement>();
  const defaultActive = () => (props.autoSelectFirstSuggestion === false ? -1 : 0);
  const id = createUniqueId();
  const listId = `${id}-listbox`;
  const token = createMemo(() => readActiveToken(props.value, caret()));
  const used = createMemo(() => collectUsedTags(props.value, props.excludedTags ?? EMPTY));
  const suggestions = createSuggestions(
    untrack(() => props.client),
    {
      tags: () => props.knownTags,
      token,
      used,
      eligible: () => focused() && open() && !props.disabled,
      excluded: () => props.excludedTags ?? EMPTY
    }
  );
  let previousItems = untrack(() => suggestions.items);
  createEffect(
    () => suggestions.items,
    (items) => {
      const previousValue = previousItems[active()]?.value;
      const match = previousValue ? items.findIndex((item) => item.value === previousValue) : -1;
      setActive(
        items.length === 0
          ? defaultActive()
          : match >= 0
            ? match
            : active() < 0
              ? defaultActive()
              : Math.min(active(), items.length - 1)
      );
      previousItems = items;
    }
  );
  createEffect(
    () => ({ focused: focused(), value: props.value, disabled: props.disabled }),
    (state) => {
      if (!state.focused || !state.value.trim() || state.disabled) setOpen(false);
    }
  );
  const sessionOpen = () => focused() && open() && !!props.value.trim() && !props.disabled;
  const popup = () => sessionOpen() && suggestions.items.length > 0;
  let frame: number | null = null;
  const schedule = (run: () => void) => {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      frame = null;
      run();
    });
  };
  onCleanup(() => {
    if (frame !== null) cancelAnimationFrame(frame);
  });
  function pick(tag: string, index = 0) {
    if (props.onSuggestionPick) {
      props.onSuggestionPick(tag);
      const keep = !!props.keepSuggestionsOpenOnPick && focused() && !!props.value.trim();
      setOpen(keep);
      setActive(keep ? index : defaultActive());
      if (props.keepSuggestionsOpenOnPick) schedule(() => input()?.focus());
      return;
    }
    const currentToken = token();
    if (!currentToken) return;
    const { nextValue, nextCaret } = applySuggestionToValue(props.value, currentToken, tag);
    props.onValueChange(nextValue);
    setOpen(false);
    setActive(defaultActive());
    schedule(() => {
      input()?.focus();
      input()?.setSelectionRange(nextCaret, nextCaret);
      setCaret(nextCaret);
    });
  }
  const updateCaret = () => setCaret(input()?.selectionStart ?? props.value.length);
  function keydown(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
    const items = suggestions.items;
    if (open() && items.length > 0) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setActive((previous) =>
          previous < 0
            ? event.key === "ArrowDown"
              ? 0
              : items.length - 1
            : (previous + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length
        );
        return;
      }
      if (event.key === "Enter") {
        const item = items[active()];
        if (item) {
          event.preventDefault();
          pick(item.value, active());
          return;
        }
      }
      if (event.key === "Escape") {
        event.stopPropagation();
        setOpen(false);
        return;
      }
    }
    if (event.key === "Enter") {
      event.preventDefault();
      props.onSubmit?.();
    }
  }
  return (
    <div class={`relative min-w-0 ${props.wrapperClassName ?? ""}`}>
      <input
        id={props.inputId ?? `${id}-input`}
        ref={(node) => {
          setInput(node);
          props.inputRef?.(node);
        }}
        class={props.inputClassName ?? "w-full"}
        value={props.value}
        disabled={props.disabled}
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck={false}
        aria-keyshortcuts={props.ariaKeyShortcuts}
        aria-label={props.ariaLabel}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={popup() ? "true" : "false"}
        aria-controls={popup() ? listId : undefined}
        aria-activedescendant={
          popup() && active() >= 0 ? `${listId}-option-${active()}` : undefined
        }
        onFocus={() => {
          setFocused(true);
          updateCaret();
          setOpen(!!props.value.trim());
        }}
        onClick={updateCaret}
        onSelect={updateCaret}
        onInput={(event) => {
          const value = event.currentTarget.value;
          props.onValueChange(value);
          setCaret(event.currentTarget.selectionStart ?? value.length);
          setOpen(!!value.trim());
          setActive(defaultActive());
        }}
        onKeyDown={keydown}
        onBlur={() => {
          setFocused(false);
          setOpen(false);
        }}
        placeholder={props.placeholder ?? t("search.placeholder")}
      />
      {suggestions.failed && focused() ? (
        <button type="button" onClick={suggestions.retry}>
          {t("search.suggestionsRetry")}
        </button>
      ) : null}
      {sessionOpen() ? (
        <SearchSuggestionsList
          id={listId}
          suggestions={suggestions.items}
          activeSuggestionIdx={active()}
          listboxAriaLabel={props.listboxAriaLabel ?? t("search.tagSuggestions")}
          placement={props.suggestionsPlacement ?? "below"}
          strategy={props.suggestionsStrategy ?? "inline"}
          anchorElement={input()}
          onPick={pick}
        />
      ) : null}
    </div>
  );
}
