import { useCallback, useRef, type ChangeEvent, type Dispatch, type KeyboardEvent, type MutableRefObject, type RefObject, type SetStateAction } from "react";
import { applySuggestionToValue } from "../services/inputService";
import type { ActiveToken, TagSuggestion } from "../types";

interface UseSearchTagatorHandlersOptions {
  value: string;
  onValueChange: (value: string) => void;
  onSubmit?: () => void;
  onSuggestionPick?: (value: string) => void;
  inputRef?: MutableRefObject<HTMLInputElement | null> | RefObject<HTMLInputElement>;
  keepSuggestionsOpenOnPick: boolean;
  activeToken: ActiveToken | null;
  suggestions: TagSuggestion[];
  suggestionsOpen: boolean;
  activeSuggestionIdx: number;
  defaultActiveSuggestionIdx: number;
  inputFocused: boolean;
  setInputFocused: Dispatch<SetStateAction<boolean>>;
  setSuggestionsOpen: Dispatch<SetStateAction<boolean>>;
  setActiveSuggestionIdx: Dispatch<SetStateAction<number>>;
  setCaretPosition: Dispatch<SetStateAction<number>>;
}

function shouldOpenSuggestions(inputValue: string): boolean {
  return inputValue.trim().length > 0;
}

export function useSearchTagatorHandlers({
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
}: UseSearchTagatorHandlersOptions) {
  const localInputRef = useRef<HTMLInputElement | null>(null);

  const setInputNode = useCallback(
    (node: HTMLInputElement | null) => {
      localInputRef.current = node;
      if (inputRef) {
        (inputRef as MutableRefObject<HTMLInputElement | null>).current = node;
      }
    },
    [inputRef]
  );

  const updateCaret = useCallback(() => {
    const input = localInputRef.current;
    if (!input) {
      return;
    }

    setCaretPosition(input.selectionStart ?? value.length);
  }, [setCaretPosition, value.length]);

  const applySuggestion = useCallback(
    (tag: string, pickedIndex = 0) => {
      if (onSuggestionPick) {
        onSuggestionPick(tag);
        const keepOpen = keepSuggestionsOpenOnPick && inputFocused && shouldOpenSuggestions(value);
        setSuggestionsOpen(keepOpen);
        setActiveSuggestionIdx(keepOpen ? pickedIndex : defaultActiveSuggestionIdx);
        if (keepSuggestionsOpenOnPick) {
          requestAnimationFrame(() => {
            localInputRef.current?.focus();
          });
        }
        return;
      }

      if (!activeToken) {
        return;
      }

      const { nextValue, nextCaret } = applySuggestionToValue(value, activeToken, tag);

      onValueChange(nextValue);
      setSuggestionsOpen(false);
      setActiveSuggestionIdx(defaultActiveSuggestionIdx);

      requestAnimationFrame(() => {
        const input = localInputRef.current;
        if (!input) {
          return;
        }

        input.focus();
        input.setSelectionRange(nextCaret, nextCaret);
        setCaretPosition(nextCaret);
      });
    },
    [
      activeToken,
      defaultActiveSuggestionIdx,
      inputFocused,
      keepSuggestionsOpenOnPick,
      onSuggestionPick,
      onValueChange,
      setActiveSuggestionIdx,
      setCaretPosition,
      setSuggestionsOpen,
      value
    ]
  );

  const handleInputFocus = useCallback(() => {
    setInputFocused(true);
    updateCaret();
    setSuggestionsOpen(shouldOpenSuggestions(value));
  }, [setInputFocused, setSuggestionsOpen, updateCaret, value]);

  const handleInputChange = useCallback(
    (event: ChangeEvent<HTMLInputElement>) => {
      const nextValue = event.target.value;
      onValueChange(nextValue);
      setCaretPosition(event.target.selectionStart ?? nextValue.length);
      setSuggestionsOpen(shouldOpenSuggestions(nextValue));
      setActiveSuggestionIdx(defaultActiveSuggestionIdx);
    },
    [defaultActiveSuggestionIdx, onValueChange, setActiveSuggestionIdx, setCaretPosition, setSuggestionsOpen]
  );

  const handleInputKeyDown = useCallback(
    (event: KeyboardEvent<HTMLInputElement>) => {
      if (suggestionsOpen && suggestions.length > 0) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setActiveSuggestionIdx((prev) => {
            if (prev < 0) {
              return 0;
            }
            return (prev + 1) % suggestions.length;
          });
          return;
        }

        if (event.key === "ArrowUp") {
          event.preventDefault();
          setActiveSuggestionIdx((prev) => {
            if (prev < 0) {
              return suggestions.length - 1;
            }
            return (prev - 1 + suggestions.length) % suggestions.length;
          });
          return;
        }

        if (event.key === "Enter") {
          const pickedSuggestion = suggestions[activeSuggestionIdx];
          if (pickedSuggestion) {
            event.preventDefault();
            applySuggestion(pickedSuggestion.value, activeSuggestionIdx);
            return;
          }
        }

        if (event.key === "Escape") {
          setSuggestionsOpen(false);
        }
      }

      if (event.key === "Enter") {
        event.preventDefault();
        onSubmit?.();
      }
    },
    [activeSuggestionIdx, applySuggestion, onSubmit, setActiveSuggestionIdx, setSuggestionsOpen, suggestions, suggestionsOpen]
  );

  const handleInputBlur = useCallback(() => {
    setInputFocused(false);
    setSuggestionsOpen(false);
  }, [setInputFocused, setSuggestionsOpen]);

  return {
    setInputNode,
    updateCaret,
    applySuggestion,
    handleInputFocus,
    handleInputChange,
    handleInputKeyDown,
    handleInputBlur
  };
}
