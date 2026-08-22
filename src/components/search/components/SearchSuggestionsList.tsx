import type { TagSuggestion } from "../types";
import { SearchSuggestionOption } from "./SearchSuggestionOption";

interface SearchSuggestionsListProps {
  id: string;
  suggestions: TagSuggestion[];
  activeSuggestionIdx: number;
  listboxAriaLabel: string;
  placement: "above" | "below";
  onPick: (suggestionValue: string, index: number) => void;
}

export function SearchSuggestionsList({
  id,
  suggestions,
  activeSuggestionIdx,
  listboxAriaLabel,
  placement,
  onPick
}: SearchSuggestionsListProps) {
  return (
    <div
      id={id}
      className={`absolute left-0 right-0 z-10 grid gap-0.5 rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-1.5 shadow-[var(--shadow-popover)] backdrop-blur-xl ${
        placement === "above" ? "bottom-[calc(100%+8px)]" : "top-[calc(100%+8px)]"
      }`}
      role="listbox"
      aria-label={listboxAriaLabel}
    >
      {suggestions.map((suggestion, index) => (
        <SearchSuggestionOption
          key={suggestion.value}
          id={`${id}-option-${index}`}
          suggestion={suggestion}
          index={index}
          active={index === activeSuggestionIdx}
          onPick={onPick}
        />
      ))}
    </div>
  );
}
