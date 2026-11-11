import { createMemo, For } from "solid-js";
import {
  getTagMode,
  type TagSelectionMode,
  type TagSelections
} from "./tagListSelectionService";
interface TagListChipListProps {
  tags: string[];
  selections: TagSelections;
  disabled: boolean;
  onTagClick: (tag: string, clickCount: number) => void;
  onTagDoubleClick: (tag: string) => void;
  getStateLabel: (mode: TagSelectionMode | null) => string;
  getButtonTitle: (tag: string, state: string) => string;
  getButtonAriaLabel: (tag: string, state: string) => string;
}
export function TagListChipList(props: TagListChipListProps) {
  return (
    <div class="flex flex-wrap gap-1.5">
      <For each={props.tags}>
        {(tag) => {
          // Memos cut the selection signal down to this chip's own mode, so a click only
          // recomputes the translated strings of chips whose mode (or the language) changed.
          const tagMode = createMemo(() => getTagMode(props.selections, tag));
          const stateLabel = createMemo(() => props.getStateLabel(tagMode()));
          const ariaLabel = createMemo(() => props.getButtonAriaLabel(tag, stateLabel()));
          const title = createMemo(() => props.getButtonTitle(tag, stateLabel()));
          const tagClasses = () =>
            [
              "inline-flex break-all items-center rounded-[var(--radius-control)] border px-3 py-1 text-xs font-medium focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-primary/35",
              tagMode() === "include"
                ? "border-primary/45 bg-primary/12 text-primary-text hover:bg-primary/20"
                : tagMode() === "exclude"
                  ? "border-error/45 bg-error/12 text-error hover:border-error/60 hover:bg-error/18"
                  : "border-base-content/12 bg-base-100 text-base-content hover:border-base-content/24 hover:bg-base-100/80"
            ]
              .filter(Boolean)
              .join(" ");
          return (
            <button
              type="button"
              class={tagClasses()}
              aria-pressed={tagMode() !== null ? "true" : "false"}
              aria-keyshortcuts="Shift+Enter Shift+Space"
              aria-label={ariaLabel()}
              title={title()}
              disabled={props.disabled}
              onClick={(event) => props.onTagClick(tag, event.detail)}
              onDblClick={() => props.onTagDoubleClick(tag)}
              onKeyDown={(event) => {
                if (!event.shiftKey || (event.key !== "Enter" && event.key !== " ")) return;
                event.preventDefault();
                props.onTagDoubleClick(tag);
              }}
            >
              {tag}
            </button>
          );
        }}
      </For>
    </div>
  );
}
