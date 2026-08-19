import { useEffect, useMemo, useState } from "react";
import type { MutableRefObject, RefObject } from "react";
import { SearchSuggestionsList } from "./components/SearchSuggestionsList";
import { useSearchTagatorHandlers } from "./hooks/useSearchTagatorHandlers";
import { buildTagSuggestions, createTagFuse } from "./services/suggestionService";
import { collectUsedTags, readActiveToken } from "./services/tokenService";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";
import type Fuse from "fuse.js";

interface SearchTagatorProps {
  value: string;
  onValueChange: (value: string) => void;
  knownTags: string[];
  onSubmit?: () => void;
  onSuggestionPick?: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  listboxAriaLabel?: string;
  inputId?: string;
  inputRef?: MutableRefObject<HTMLInputElement | null> | RefObject<HTMLInputElement>;
  excludedTags?: string[];
  inputClassName?: string;
  keepSuggestionsOpenOnPick?: boolean;
  autoSelectFirstSuggestion?: boolean;
  suggestionsPlacement?: "above" | "below";
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
  listboxAriaLabel,
  inputId,
  inputRef,
  excludedTags = [],
  inputClassName = "w-full",
  keepSuggestionsOpenOnPick = false,
  autoSelectFirstSuggestion = true,
  suggestionsPlacement = "below",
  disabled = false
}: SearchTagatorProps) {
  const { t } = useTranslation();
  const [caretPosition, setCaretPosition] = useState(0);
  const defaultActiveSuggestionIdx = autoSelectFirstSuggestion ? 0 : -1;
  const [activeSuggestionIdx, setActiveSuggestionIdx] = useState(defaultActiveSuggestionIdx);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [inputFocused, setInputFocused] = useState(false);
  const [FuseClass, setFuseClass] = useState<typeof Fuse | null>(null);

  const activeToken = useMemo(() => readActiveToken(value, caretPosition), [value, caretPosition]);

  const usedTags = useMemo(() => collectUsedTags(value, excludedTags), [excludedTags, value]);
  useEffect(() => {
    if (!inputFocused || FuseClass) return;
    let active = true;
    void import("fuse.js").then((module) => {
      if (active) setFuseClass(() => module.default);
    });
    return () => {
      active = false;
    };
  }, [FuseClass, inputFocused]);

  const fuse = useMemo(
    () => (FuseClass ? createTagFuse(knownTags, FuseClass) : null),
    [FuseClass, knownTags]
  );
  const suggestions = useMemo(
    () => (fuse ? buildTagSuggestions({ activeToken, usedTags, fuse }) : []),
    [activeToken, fuse, usedTags]
  );

  useEffect(() => {
    if (!inputFocused || !shouldOpenSuggestions(value)) {
      setSuggestionsOpen(false);
    }
  }, [inputFocused, value]);

  useEffect(() => {
    if (suggestions.length === 0) {
      setActiveSuggestionIdx(defaultActiveSuggestionIdx);
      return;
    }

    setActiveSuggestionIdx((prev) => {
      if (prev < 0) {
        return autoSelectFirstSuggestion ? 0 : -1;
      }

      return Math.min(prev, suggestions.length - 1);
    });
  }, [autoSelectFirstSuggestion, defaultActiveSuggestionIdx, suggestions.length]);

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
        id={inputId}
        ref={handlers.setInputNode}
        className={inputClassName}
        value={value}
        disabled={disabled}
        {...browserAssistDisabledProps}
        aria-label={ariaLabel}
        onFocus={handlers.handleInputFocus}
        onClick={handlers.updateCaret}
        onSelect={handlers.updateCaret}
        onChange={handlers.handleInputChange}
        onKeyDown={handlers.handleInputKeyDown}
        onBlur={handlers.handleInputBlur}
        placeholder={placeholder ?? t("search.placeholder")}
      />

      {inputFocused && shouldOpenSuggestions(value) && suggestionsOpen && suggestions.length > 0 ? (
        <SearchSuggestionsList
          suggestions={suggestions}
          activeSuggestionIdx={activeSuggestionIdx}
          listboxAriaLabel={listboxAriaLabel ?? t("search.tagSuggestions")}
          placement={suggestionsPlacement}
          onPick={handlers.applySuggestion}
        />
      ) : null}
    </div>
  );
}
