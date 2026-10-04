import { renderHighlightedMatch } from "./renderHighlightedMatch";
import type { TagSuggestion } from "../../../components/search/types";
interface SearchSuggestionOptionProps {
  id: string;
  suggestion: TagSuggestion;
  index: number;
  active: boolean;
  onPick: (suggestionValue: string, index: number) => void;
}
export function SearchSuggestionOption(props: SearchSuggestionOptionProps) {
  const activeStyle = () =>
    props.active
      ? {
          "background-color": "var(--tagator-suggestion-active-bg)",
          "box-shadow": "inset 0 0 0 1px var(--tagator-suggestion-active-ring)"
        }
      : undefined;
  return (
    <button
      type="button"
      id={props.id}
      role="option"
      tabindex={-1}
      aria-selected={props.active ? "true" : "false"}
      class="w-full rounded-[var(--radius-control)] border-0 bg-transparent px-2.5 py-[7px] text-left text-base-content hover:bg-base-content/12 focus-visible:bg-base-content/12 focus-visible:outline-hidden"
      style={activeStyle()}
      onMouseDown={(event) => {
        event.preventDefault();
        props.onPick(props.suggestion.value, props.index);
      }}
    >
      {renderHighlightedMatch(props.suggestion.value, props.suggestion.indices)}
    </button>
  );
}
