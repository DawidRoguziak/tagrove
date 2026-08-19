import { renderHighlightedMatch } from "../renderHighlightedMatch";
import type { TagSuggestion } from "../types";

interface SearchSuggestionOptionProps {
  suggestion: TagSuggestion;
  index: number;
  active: boolean;
  onPick: (suggestionValue: string, index: number) => void;
}

export function SearchSuggestionOption({
  suggestion,
  index,
  active,
  onPick
}: SearchSuggestionOptionProps) {
  const activeStyle = active
    ? {
        backgroundColor: "var(--tagator-suggestion-active-bg)",
        boxShadow: "inset 0 0 0 1px var(--tagator-suggestion-active-ring)"
      }
    : undefined;

  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      className="w-full rounded-lg border-0 bg-transparent px-2.5 py-[7px] text-left text-base-content transition-colors duration-100 hover:bg-base-content/12 focus-visible:bg-base-content/12 focus-visible:outline-hidden"
      style={activeStyle}
      onMouseDown={(event) => {
        event.preventDefault();
        onPick(suggestion.value, index);
      }}
    >
      {renderHighlightedMatch(suggestion.value, suggestion.indices)}
    </button>
  );
}
