import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { GalleryScrollbar } from "../GalleryScrollbar";

// jsdom has no PointerEvent; the shim carries the fields the bar reads.
class TestPointerEvent extends MouseEvent {
  pointerId: number;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
  }
}
beforeEach(() => {
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
    configurable: true,
    value: vi.fn()
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
// 600px viewport: the track is 592px tall, the thumb its 32px minimum, travel 560px.
function mount() {
  const onScrollTo = vi.fn();
  const [offset, setOffset] = createSignal(0);
  render(() => (
    <GalleryScrollbar
      controls="grid"
      label="Gallery position"
      viewport={600}
      offset={offset()}
      maxOffset={1_000_000}
      stride={200}
      firstVisible={Math.floor(offset() / 200) * 5}
      count={25_000}
      onScrollTo={(value) => {
        onScrollTo(value);
        setOffset(Math.min(1_000_000, Math.max(0, value)));
      }}
    />
  ));
  const bar = screen.getByRole("scrollbar", { name: "Gallery position" });
  vi.spyOn(bar, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 4, 10, 592));
  const thumb = bar.querySelector<HTMLElement>("[data-gallery-scroll-thumb]");
  if (!thumb) throw new Error("Missing thumb");
  return { bar, thumb, onScrollTo, setOffset };
}

it("exposes an accessible global position and moves one thumb transform", () => {
  const { bar, thumb, setOffset } = mount();
  expect(bar).toHaveAttribute("aria-orientation", "vertical");
  expect(bar).toHaveAttribute("aria-controls", "grid");
  expect(bar).toHaveAttribute("tabindex", "0");
  expect(bar).toHaveAttribute("aria-valuemin", "1");
  expect(bar).toHaveAttribute("aria-valuemax", "25000");
  expect(bar).toHaveAttribute("aria-valuenow", "1");
  expect(bar.style.height).toBe("592px");
  expect(thumb.style.height).toBe("32px");
  setOffset(500_000);
  flush();
  expect(thumb.style.transform).toBe("translate(0, 280px)");
  expect(bar).toHaveAttribute("aria-valuenow", "12501");
});

it("jumps to a track press and keeps dragging from the jumped position", () => {
  const { bar, onScrollTo } = mount();
  fireEvent.pointerDown(bar, { button: 0, pointerId: 1, clientY: 4 + 296 });
  flush();
  // Thumb centre at the pointer: (296 - 16) / 560 = half the range.
  expect(onScrollTo).toHaveBeenLastCalledWith(500_000);
  fireEvent.pointerMove(bar, { pointerId: 1, clientY: 4 + 296 + 56 });
  expect(onScrollTo).toHaveBeenLastCalledWith(600_000);
  fireEvent.pointerUp(bar, { pointerId: 1 });
  fireEvent.pointerMove(bar, { pointerId: 1, clientY: 590 });
  expect(onScrollTo).toHaveBeenCalledTimes(2);
});

it("drags the thumb relative to its grab point and clamps to the ends", () => {
  const { bar, onScrollTo } = mount();
  fireEvent.pointerDown(bar, { button: 0, pointerId: 2, clientY: 4 + 10 });
  expect(onScrollTo).not.toHaveBeenCalled();
  fireEvent.pointerMove(bar, { pointerId: 2, clientY: 4 + 10 + 2000 });
  expect(onScrollTo).toHaveBeenLastCalledWith(1_000_000);
  fireEvent.pointerMove(bar, { pointerId: 2, clientY: -2000 });
  expect(onScrollTo).toHaveBeenLastCalledWith(0);
});

it("scrolls by row, page and to the ends from the keyboard", () => {
  const { bar, onScrollTo, setOffset } = mount();
  setOffset(10_000);
  flush();
  const keys: [string, number][] = [
    ["ArrowDown", 10_200],
    ["ArrowUp", 9_800],
    ["PageDown", 10_600],
    ["PageUp", 9_400],
    ["Home", Number.NEGATIVE_INFINITY],
    ["End", Number.POSITIVE_INFINITY]
  ];
  for (const [key, expected] of keys) {
    setOffset(10_000);
    flush();
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    bar.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(onScrollTo).toHaveBeenLastCalledWith(expected);
  }
  const other = new KeyboardEvent("keydown", { key: "a", bubbles: true, cancelable: true });
  bar.dispatchEvent(other);
  expect(other.defaultPrevented).toBe(false);
});
