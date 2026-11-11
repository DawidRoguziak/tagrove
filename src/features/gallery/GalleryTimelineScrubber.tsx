import { createEffect, createMemo, createSignal, For, onCleanup, Show, untrack } from "solid-js";
import {
  indexAtFraction,
  monthAt,
  timelineKeyTarget,
  type TimelineKey,
  type TimelineMonth,
  yearOf
} from "./timeline";

/** Hit width of the rail in px; the grid reserves it (plus the right offset) as padding. */
export const TIMELINE_WIDTH = 10;
const INSET = 4;
const MIN_TICK_GAP = 14;
const KEYS = new Set<string>(["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End"]);

/**
 * Month timeline at the right edge of the gallery (stage 08). Track position is
 * `index / total`; the grid maps an index to its row offset in O(1). A drag moves the
 * grid once per animation frame and holds page and thumbnail demand until release or a
 * short rest. It sits beside the position bar (native or `GalleryScrollbar`), not over it.
 */
export function GalleryTimelineScrubber(props: {
  controls: string;
  label: string;
  months: readonly TimelineMonth[];
  total: number;
  viewport: number;
  firstVisible: number;
  /** Right offset in px, so the rail clears the position bar. */
  right: number;
  formatMonth: (t: number) => string;
  onJump: (index: number) => void;
  onHold: () => void;
  onRelease: () => void;
}) {
  const track = () => Math.max(0, props.viewport - 2 * INSET);
  // The index the timeline last moved to. Near the end of the result the scroll clamps, so
  // the first visible item can still belong to an earlier month; until the view moves for
  // another reason, the timeline reports (and steps from) its own target instead.
  const [cursor, setCursor] = createSignal<number | null>(null);
  let settled: number | null = null;
  let settleFrame = 0;
  const position = () => cursor() ?? props.firstVisible;
  createEffect(
    () => props.firstVisible,
    (first) => {
      if (settled !== null && first !== settled) {
        settled = null;
        setCursor(null);
      }
    }
  );
  const moveTo = (index: number) => {
    setCursor(index);
    settled = null;
    cancelAnimationFrame(settleFrame);
    props.onJump(index);
    // The jump's own scroll publishes the new first visible item before the next frame.
    settleFrame = requestAnimationFrame(() => {
      settled = untrack(() => props.firstVisible);
    });
  };
  const current = createMemo(() => monthAt(props.months, position()));
  const currentText = () => {
    const month = props.months[current()];
    return month ? props.formatMonth(month.t) : "";
  };
  const fractionOf = (index: number) => (props.total > 0 ? index / props.total : 0);
  // Year boundaries, thinned so labels never overlap on a short track.
  const ticks = createMemo(() => {
    const result: { year: number; y: number }[] = [];
    let lastY = Number.NEGATIVE_INFINITY;
    props.months.forEach((month, index) => {
      if (month.t <= 0) return;
      if (index > 0 && yearOf(props.months[index - 1].t) === yearOf(month.t)) return;
      const y = fractionOf(month.start) * track();
      if (y - lastY < MIN_TICK_GAP) return;
      lastY = y;
      result.push({ year: yearOf(month.t), y });
    });
    return result;
  });
  const [hover, setHover] = createSignal<{ y: number; text: string } | null>(null);
  const [dragging, setDragging] = createSignal(false);
  const [focused, setFocused] = createSignal(false);
  let pointerId: number | null = null;
  let pending: number | null = null;
  let frame = 0;
  onCleanup(() => {
    cancelAnimationFrame(frame);
    cancelAnimationFrame(settleFrame);
    if (pointerId !== null) props.onRelease();
  });
  const indexAt = (element: HTMLElement, clientY: number) => {
    const box = element.getBoundingClientRect();
    return indexAtFraction((clientY - box.top) / Math.max(1, box.height), props.total);
  };
  const describe = (element: HTMLElement, clientY: number) => {
    const index = indexAt(element, clientY);
    const month = props.months[monthAt(props.months, index)];
    const box = element.getBoundingClientRect();
    setHover({
      y: Math.min(track(), Math.max(0, clientY - box.top)),
      text: month ? props.formatMonth(month.t) : ""
    });
    return index;
  };
  // Pointer moves arrive faster than frames; only the latest target is applied.
  const jumpInFrame = (index: number) => {
    pending = index;
    props.onHold();
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pending !== null) moveTo(pending);
      pending = null;
    });
  };
  const endDrag = () => {
    if (pointerId === null) return;
    pointerId = null;
    setDragging(false);
    setHover(null);
    props.onRelease();
  };
  return (
    <div class="pointer-events-none sticky top-0 z-10 h-0" data-testid="gallery-timeline">
      <div
        role="slider"
        aria-controls={props.controls}
        aria-orientation="vertical"
        aria-label={props.label}
        aria-valuemin={1}
        aria-valuemax={props.months.length}
        aria-valuenow={current() + 1}
        aria-valuetext={currentText()}
        tabindex="0"
        data-gallery-timeline-dragging={dragging() ? "" : undefined}
        class="group pointer-events-auto absolute touch-none rounded-full outline-offset-2 hover:bg-base-content/5 focus-visible:outline-2 focus-visible:outline-primary"
        style={{
          top: `${INSET}px`,
          width: `${TIMELINE_WIDTH}px`,
          height: `${track()}px`,
          right: `${props.right}px`
        }}
        onPointerDown={(event) => {
          if (event.button !== 0 || props.total <= 0) return;
          event.preventDefault();
          // preventDefault also cancels the focus change in WebKitGTK; take it explicitly so
          // the arrow and page keys that follow a click move the timeline.
          event.currentTarget.focus({ preventScroll: true });
          pointerId = event.pointerId;
          setDragging(true);
          event.currentTarget.setPointerCapture(event.pointerId);
          jumpInFrame(describe(event.currentTarget, event.clientY));
        }}
        onPointerMove={(event) => {
          if (pointerId === null) {
            if (event.pointerType === "mouse") describe(event.currentTarget, event.clientY);
            return;
          }
          if (pointerId !== event.pointerId) return;
          jumpInFrame(describe(event.currentTarget, event.clientY));
        }}
        onPointerLeave={() => {
          if (pointerId === null) setHover(null);
        }}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onLostPointerCapture={endDrag}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(event) => {
          if (!KEYS.has(event.key) || event.altKey || event.ctrlKey || event.metaKey) return;
          event.preventDefault();
          const target = timelineKeyTarget(
            event.key as TimelineKey,
            props.months,
            untrack(position),
            props.total
          );
          if (target !== null) moveTo(target);
        }}
      >
        <For each={ticks()}>
          {(tick) => (
            <div
              aria-hidden="true"
              class="pointer-events-none absolute right-0 h-px w-2 bg-base-content/30"
              style={{ transform: `translate(0, ${tick.y}px)` }}
            >
              <span class="absolute right-3 -top-1.5 hidden whitespace-nowrap font-mono text-[10px] leading-none text-base-content/60 group-hover:block">
                {tick.year}
              </span>
            </div>
          )}
        </For>
        <div
          aria-hidden="true"
          data-gallery-timeline-marker
          class="pointer-events-none absolute inset-x-0 h-1 rounded-full bg-primary/70"
          style={{ transform: `translate(0, ${fractionOf(position()) * track()}px)` }}
        />
        <Show when={hover() ?? (focused() ? { y: fractionOf(position()) * track(), text: currentText() } : null)}>
          {(label) => (
            <div
              aria-hidden="true"
              data-testid="gallery-timeline-label"
              class="pointer-events-none absolute right-5 -translate-y-1/2 whitespace-nowrap rounded-[var(--radius-surface)] border border-[var(--border-strong)] bg-[var(--surface-solid)] px-2 py-1 text-xs shadow-[var(--shadow-popover)]"
              style={{ top: `${label().y}px` }}
            >
              {label().text}
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
