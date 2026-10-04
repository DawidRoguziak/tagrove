import { createEffect, createSignal, For } from "solid-js";
import { Portal, type JSX } from "@solidjs/web";
import type { TagSuggestion } from "../../../components/search/types";
import { SearchSuggestionOption } from "./SearchSuggestionOption";
const VIEWPORT_EDGE_GAP = 12;
const INPUT_LIST_GAP = 8;
const MAX_LIST_HEIGHT = 288;
type SuggestionsStrategy = "inline" | "viewport";
interface ViewportListStyle extends JSX.CSSProperties {
  "max-height": string;
}
interface SearchSuggestionsListProps {
  id: string;
  suggestions: TagSuggestion[];
  activeSuggestionIdx: number;
  listboxAriaLabel: string;
  placement: "above" | "below";
  strategy: SuggestionsStrategy;
  anchorElement: HTMLInputElement | undefined;
  onPick: (suggestionValue: string, index: number) => void;
}
export function SearchSuggestionsList(props: SearchSuggestionsListProps) {
  const [viewportStyle, setViewportStyle] = createSignal<ViewportListStyle | null>(null);
  createEffect(
    () => ({ anchor: props.anchorElement, placement: props.placement, strategy: props.strategy }),
    ({ anchor, placement, strategy }) => {
      if (strategy !== "viewport" || !anchor) {
        setViewportStyle(null);
        return;
      }
      const updatePosition = () => {
        const rect = anchor.getBoundingClientRect();
        const viewportWidth = window.innerWidth;
        const viewportHeight = window.innerHeight;
        const width = Math.max(0, Math.min(rect.width, viewportWidth - VIEWPORT_EDGE_GAP * 2));
        const left = Math.min(
          Math.max(VIEWPORT_EDGE_GAP, rect.left),
          Math.max(VIEWPORT_EDGE_GAP, viewportWidth - VIEWPORT_EDGE_GAP - width)
        );
        const availableHeight =
          placement === "above"
            ? rect.top - INPUT_LIST_GAP - VIEWPORT_EDGE_GAP
            : viewportHeight - rect.bottom - INPUT_LIST_GAP - VIEWPORT_EDGE_GAP;
        const nextStyle: ViewportListStyle = {
          left: `${left}px`,
          width: `${width}px`,
          "max-height": `${Math.max(0, Math.min(MAX_LIST_HEIGHT, availableHeight))}px`
        };
        if (placement === "above") {
          nextStyle.bottom = `${viewportHeight - rect.top + INPUT_LIST_GAP}px`;
        } else {
          nextStyle.top = `${rect.bottom + INPUT_LIST_GAP}px`;
        }
        setViewportStyle(nextStyle);
      };
      updatePosition();
      const observer = new ResizeObserver(updatePosition);
      observer.observe(anchor);
      window.addEventListener("resize", updatePosition);
      window.addEventListener("scroll", updatePosition, true);
      window.visualViewport?.addEventListener("resize", updatePosition);
      return () => {
        observer.disconnect();
        window.removeEventListener("resize", updatePosition);
        window.removeEventListener("scroll", updatePosition, true);
        window.visualViewport?.removeEventListener("resize", updatePosition);
      };
    }
  );
  const list = () => (
    <div
      id={props.id}
      class={[
        props.strategy !== "viewport" || viewportStyle() ? "motion-enter" : "",
        "grid gap-0.5 overflow-y-auto rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-raised)] p-1.5 shadow-[var(--shadow-popover)]",
        props.strategy === "viewport"
          ? "fixed z-[70]"
          : `absolute left-0 right-0 z-10 max-h-[min(18rem,45vh)] ${props.placement === "above" ? "bottom-[calc(100%+8px)]" : "top-[calc(100%+8px)]"}`
      ].join(" ")}
      style={{
        ...(props.strategy === "viewport" ? viewportStyle() : undefined),
        visibility:
          props.suggestions.length === 0 || (props.strategy === "viewport" && !viewportStyle())
            ? "hidden"
            : undefined,
        "animation-play-state": props.suggestions.length === 0 ? "paused" : undefined
      }}
      role="listbox"
      aria-label={props.listboxAriaLabel}
    >
      <For each={props.suggestions} keyed={false}>
        {(suggestion, index) => (
          <SearchSuggestionOption
            id={`${props.id}-option-${index}`}
            suggestion={suggestion()}
            index={index}
            active={index === props.activeSuggestionIdx}
            onPick={props.onPick}
          />
        )}
      </For>
    </div>
  );
  return <>{props.strategy === "viewport" ? <Portal>{list()}</Portal> : list()}</>;
}
