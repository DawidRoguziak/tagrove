const MIN_THUMB = 32;
const INSET = 4;

/**
 * Global position bar for grids taller than the physical scroll window. The native bar
 * would only span the window, so the scroller hides it in that mode (`styles.css`).
 * Offsets are the grid's logical pixels; the bar never reads layout while scrolling.
 */
export function GalleryScrollbar(props: {
  controls: string;
  label: string;
  viewport: number;
  offset: number;
  maxOffset: number;
  stride: number;
  firstVisible: number;
  count: number;
  onScrollTo: (offset: number) => void;
}) {
  const track = () => Math.max(0, props.viewport - 2 * INSET);
  const thumb = () =>
    Math.min(
      track(),
      Math.max(MIN_THUMB, (track() * props.viewport) / (Math.max(0, props.maxOffset) + props.viewport))
    );
  const travel = () => Math.max(1, track() - thumb());
  const fraction = () =>
    props.maxOffset > 0 ? Math.min(1, Math.max(0, props.offset / props.maxOffset)) : 0;
  const scrollToFraction = (value: number) =>
    props.onScrollTo(Math.min(1, Math.max(0, value)) * Math.max(0, props.maxOffset));
  let drag: { pointerId: number; startY: number; startFraction: number } | null = null;
  return (
    <div class="pointer-events-none sticky top-0 z-10 h-0" data-testid="gallery-scrollbar">
      <div
        role="scrollbar"
        aria-controls={props.controls}
        aria-orientation="vertical"
        aria-label={props.label}
        aria-valuemin={1}
        aria-valuemax={props.count}
        aria-valuenow={Math.min(props.count, props.firstVisible + 1)}
        tabindex="0"
        class="pointer-events-auto absolute right-1 w-2.5 touch-none rounded-full bg-base-content/5 outline-offset-2 hover:bg-base-content/10 focus-visible:outline-2 focus-visible:outline-primary"
        style={{ top: `${INSET}px`, height: `${track()}px` }}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          const box = event.currentTarget.getBoundingClientRect();
          const y = event.clientY - box.top;
          let start = fraction();
          const top = start * travel();
          // A track press moves the thumb centre to the pointer, then drags from there.
          if (y < top || y > top + thumb()) {
            start = Math.min(1, Math.max(0, (y - thumb() / 2) / travel()));
            scrollToFraction(start);
          }
          drag = { pointerId: event.pointerId, startY: event.clientY, startFraction: start };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          if (!drag || drag.pointerId !== event.pointerId) return;
          scrollToFraction(drag.startFraction + (event.clientY - drag.startY) / travel());
        }}
        onPointerUp={() => {
          drag = null;
        }}
        onPointerCancel={() => {
          drag = null;
        }}
        onLostPointerCapture={() => {
          drag = null;
        }}
        onKeyDown={(event) => {
          const step =
            event.key === "ArrowDown"
              ? props.stride
              : event.key === "ArrowUp"
                ? -props.stride
                : event.key === "PageDown"
                  ? props.viewport
                  : event.key === "PageUp"
                    ? -props.viewport
                    : 0;
          if (event.key === "Home") props.onScrollTo(Number.NEGATIVE_INFINITY);
          else if (event.key === "End") props.onScrollTo(Number.POSITIVE_INFINITY);
          else if (step) props.onScrollTo(props.offset + step);
          else return;
          event.preventDefault();
        }}
      >
        <div
          class="pointer-events-none absolute inset-x-0 top-0 rounded-full bg-[var(--border-strong)]"
          data-gallery-scroll-thumb
          style={{ height: `${thumb()}px`, transform: `translate(0, ${fraction() * travel()}px)` }}
        />
      </div>
    </div>
  );
}
