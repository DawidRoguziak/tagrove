import { useLayoutEffect, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { TagSuggestion } from "../types";
import { SearchSuggestionOption } from "./SearchSuggestionOption";

const VIEWPORT_EDGE_GAP = 12;
const INPUT_LIST_GAP = 8;
const MAX_LIST_HEIGHT = 288;

type SuggestionsStrategy = "inline" | "viewport";

interface ViewportListStyle extends CSSProperties {
  maxHeight: number;
}

interface SearchSuggestionsListProps {
  id: string;
  suggestions: TagSuggestion[];
  activeSuggestionIdx: number;
  listboxAriaLabel: string;
  placement: "above" | "below";
  strategy: SuggestionsStrategy;
  anchorElement: HTMLInputElement | null;
  onPick: (suggestionValue: string, index: number) => void;
}

export function SearchSuggestionsList({
  id,
  suggestions,
  activeSuggestionIdx,
  listboxAriaLabel,
  placement,
  strategy,
  anchorElement,
  onPick
}: SearchSuggestionsListProps) {
  const [viewportStyle, setViewportStyle] = useState<ViewportListStyle | null>(null);

  useLayoutEffect(() => {
    if (strategy !== "viewport" || !anchorElement) {
      setViewportStyle(null);
      return;
    }

    const updatePosition = () => {
      const rect = anchorElement.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;
      const width = Math.max(0, Math.min(rect.width, viewportWidth - VIEWPORT_EDGE_GAP * 2));
      const left = Math.min(
        Math.max(VIEWPORT_EDGE_GAP, rect.left),
        Math.max(VIEWPORT_EDGE_GAP, viewportWidth - VIEWPORT_EDGE_GAP - width)
      );
      const availableHeight = placement === "above"
        ? rect.top - INPUT_LIST_GAP - VIEWPORT_EDGE_GAP
        : viewportHeight - rect.bottom - INPUT_LIST_GAP - VIEWPORT_EDGE_GAP;
      const nextStyle: ViewportListStyle = {
        left,
        width,
        maxHeight: Math.max(0, Math.min(MAX_LIST_HEIGHT, availableHeight))
      };

      if (placement === "above") {
        nextStyle.bottom = viewportHeight - rect.top + INPUT_LIST_GAP;
      } else {
        nextStyle.top = rect.bottom + INPUT_LIST_GAP;
      }
      setViewportStyle(nextStyle);
    };

    updatePosition();
    const observer = new ResizeObserver(updatePosition);
    observer.observe(anchorElement);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    window.visualViewport?.addEventListener("resize", updatePosition);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      window.visualViewport?.removeEventListener("resize", updatePosition);
    };
  }, [anchorElement, placement, strategy]);

  const list = (
    <div
      id={id}
      className={[
        "grid gap-0.5 overflow-y-auto rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-1.5 shadow-[var(--shadow-popover)]",
        strategy === "viewport"
          ? "fixed z-[70]"
          : `absolute left-0 right-0 z-10 max-h-[min(18rem,45vh)] ${
              placement === "above" ? "bottom-[calc(100%+8px)]" : "top-[calc(100%+8px)]"
            }`
      ].join(" ")}
      style={strategy === "viewport" ? viewportStyle ?? { visibility: "hidden" } : undefined}
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

  return strategy === "viewport" ? createPortal(list, document.body) : list;
}
