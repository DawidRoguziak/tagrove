import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { MutableRefObject, RefObject } from "react";
import { SearchSuggestionsList } from "./components/SearchSuggestionsList";
import { useSearchTagatorHandlers } from "./hooks/useSearchTagatorHandlers";
import { useSuggestions } from "./worker/SuggestionProvider";
import { collectUsedTags, readActiveToken } from "./services/tokenService";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";


interface SearchTagatorProps {
  value: string;
  onValueChange: (value: string) => void;
  knownTags: string[];
  onSubmit?: () => void;
  onSuggestionPick?: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  ariaKeyShortcuts?: string;
  listboxAriaLabel?: string;
  inputId?: string;
  inputRef?: MutableRefObject<HTMLInputElement | null> | RefObject<HTMLInputElement>;
  excludedTags?: string[];
  inputClassName?: string;
  keepSuggestionsOpenOnPick?: boolean;
  autoSelectFirstSuggestion?: boolean;
  suggestionsPlacement?: "above" | "below";
  suggestionsStrategy?: "inline" | "viewport";
  disabled?: boolean;
}

function shouldOpenSuggestions(inputValue: string): boolean {
  return inputValue.trim().length > 0;
}

export function SearchTagator({
  value,
  onValueChange,
  knownTags,
  onSubmit,
  onSuggestionPick,
  placeholder,
  ariaLabel,
  ariaKeyShortcuts,
  listboxAriaLabel,
  inputId,
  inputRef,
  excludedTags = [],
  inputClassName = "w-full",
  keepSuggestionsOpenOnPick = false,
  autoSelectFirstSuggestion = true,
  suggestionsPlacement = "below",
  suggestionsStrategy = "inline",
  disabled = false
}: SearchTagatorProps) {
  const { t } = useTranslation();
  const [caretPosition, setCaretPosition] = useState(0);
  const defaultActiveSuggestionIdx = autoSelectFirstSuggestion ? 0 : -1;
  const [activeSuggestionIdx, setActiveSuggestionIdx] = useState(defaultActiveSuggestionIdx);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [inputFocused, setInputFocused] = useState(false);

  const generatedId = useId();
  const resolvedInputId = inputId ?? `${generatedId}-input`;
  const listboxId = `${generatedId}-listbox`;

  const activeToken = useMemo(() => readActiveToken(value, caretPosition), [value, caretPosition]);

  const usedTags = useMemo(() => collectUsedTags(value, excludedTags), [excludedTags, value]);
  const { suggestions, failed: suggestionsFailed, retry: retrySuggestions } = useSuggestions(
    knownTags, activeToken, usedTags, inputFocused && suggestionsOpen && !disabled, excludedTags
  );
  const previousSuggestionsRef = useRef(suggestions);
  const inputNodeRef = useRef<HTMLInputElement | null>(null);
  const suggestionSessionOpen = inputFocused && shouldOpenSuggestions(value) && suggestionsOpen && !disabled;
  const popupOpen = suggestionSessionOpen && suggestions.length > 0;

  useEffect(() => {
    if (!inputFocused || !shouldOpenSuggestions(value)) {
      setSuggestionsOpen(false);
    }
  }, [inputFocused, value]);

  useLayoutEffect(() => {
    const previousSuggestions = previousSuggestionsRef.current;
    previousSuggestionsRef.current = suggestions;
    if (suggestions.length === 0) {
      setActiveSuggestionIdx(defaultActiveSuggestionIdx);
      return;
    }

    setActiveSuggestionIdx((prev) => {
      const previousValue = previousSuggestions[prev]?.value;
      const matchingIndex = previousValue
        ? suggestions.findIndex((suggestion) => suggestion.value === previousValue)
        : -1;
      if (matchingIndex >= 0) return matchingIndex;
      if (prev < 0) {
        return autoSelectFirstSuggestion ? 0 : -1;
      }

      return Math.min(prev, suggestions.length - 1);
    });
  }, [autoSelectFirstSuggestion, defaultActiveSuggestionIdx, suggestions]);

  const handlers = useSearchTagatorHandlers({
    value,
    onValueChange,
    onSubmit,
    onSuggestionPick,
    inputRef,
    keepSuggestionsOpenOnPick,
    activeToken,
    suggestions,
    suggestionsOpen,
    activeSuggestionIdx,
    defaultActiveSuggestionIdx,
    inputFocused,
    setInputFocused,
    setSuggestionsOpen,
    setActiveSuggestionIdx,
    setCaretPosition
  });

  return (
    <div className="relative min-w-0">
      <input
        id={resolvedInputId}
        ref={(node) => {
          inputNodeRef.current = node;
          handlers.setInputNode(node);
        }}
        className={inputClassName}
        value={value}
        disabled={disabled}
        {...browserAssistDisabledProps}
        aria-keyshortcuts={ariaKeyShortcuts}
          aria-label={ariaLabel}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={popupOpen}
        aria-controls={popupOpen ? listboxId : undefined}
        aria-activedescendant={
          popupOpen && activeSuggestionIdx >= 0
            ? `${listboxId}-option-${activeSuggestionIdx}`
            : undefined
        }
        onFocus={handlers.handleInputFocus}
        onClick={handlers.updateCaret}
        onSelect={handlers.updateCaret}
        onChange={handlers.handleInputChange}
        onKeyDown={handlers.handleInputKeyDown}
        onBlur={handlers.handleInputBlur}
        placeholder={placeholder ?? t("search.placeholder")}
      />

      {suggestionsFailed && inputFocused ? <button type="button" onClick={retrySuggestions}>{t("search.suggestionsRetry")}</button> : null}
      {/* Keep the list mounted through pending worker results so typing cannot replay its entrance. */}
      {suggestionSessionOpen ? (
        <SearchSuggestionsList
          id={listboxId}
          suggestions={suggestions}
          activeSuggestionIdx={activeSuggestionIdx}
          listboxAriaLabel={listboxAriaLabel ?? t("search.tagSuggestions")}
          placement={suggestionsPlacement}
          strategy={suggestionsStrategy}
          anchorElement={inputNodeRef.current}
          onPick={handlers.applySuggestion}
        />
      ) : null}
    </div>
  );
}
