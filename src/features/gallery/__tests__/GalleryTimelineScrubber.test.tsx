import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { GalleryTimelineScrubber } from "../GalleryTimelineScrubber";
import { formatTimelineMonth, timelineMonths } from "../timeline";

class TestPointerEvent extends MouseEvent {
  pointerId: number;
  pointerType: string;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.pointerType = init.pointerType ?? "mouse";
  }
}
let frames: FrameRequestCallback[] = [];
beforeEach(() => {
  frames = [];
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.push(callback);
    return frames.length;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
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
const runFrames = () => {
  const pending = frames;
  frames = [];
  for (const callback of pending) callback(0);
  flush();
};

// One million results over three months and a no-date tail.
const TOTAL = 1_000_000;
const months = timelineMonths([
  { t: 202403, start: 0, count: 5000 },
  { t: 202403, start: 5000, count: 495_000 },
  { t: 202402, start: 500_000, count: 250_000 },
  { t: 202312, start: 750_000, count: 249_990 },
  { t: 0, start: 999_990, count: 10 }
]);
/** `clampFrom`: the scroll cannot bring indexes beyond it to the top (end of the result). */
function mount(clampFrom = Number.POSITIVE_INFINITY) {
  const [first, setFirst] = createSignal(0);
  const onJump = vi.fn((index: number) => setFirst(Math.min(index, clampFrom)));
  const onHold = vi.fn();
  const onRelease = vi.fn();
  render(() => (
    <GalleryTimelineScrubber
      controls="grid"
      label="Timeline"
      months={months}
      total={TOTAL}
      viewport={608}
      firstVisible={first()}
      right={4}
      formatMonth={(t) => formatTimelineMonth(t, "en", "No date")}
      onJump={onJump}
      onHold={onHold}
      onRelease={onRelease}
    />
  ));
  // 600px track starting at y=4.
  const slider = screen.getByRole("slider", { name: "Timeline" });
  vi.spyOn(slider, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 4, 12, 600));
  return { slider, onJump, onHold, onRelease, setFirst };
}

it("exposes the current month as an accessible vertical slider", () => {
  const view = mount();
  expect(view.slider.getAttribute("aria-orientation")).toBe("vertical");
  expect(view.slider.getAttribute("aria-controls")).toBe("grid");
  expect(view.slider.getAttribute("aria-valuemin")).toBe("1");
  expect(view.slider.getAttribute("aria-valuemax")).toBe("4");
  expect(view.slider.getAttribute("aria-valuenow")).toBe("1");
  expect(view.slider.getAttribute("aria-valuetext")).toBe("March 2024");
  view.setFirst(999_995);
  flush();
  expect(view.slider.getAttribute("aria-valuenow")).toBe("4");
  expect(view.slider.getAttribute("aria-valuetext")).toBe("No date");
});

it("jumps by month, year and end with the keyboard", () => {
  const view = mount();
  fireEvent.keyDown(view.slider, { key: "ArrowDown" });
  expect(view.onJump).toHaveBeenLastCalledWith(500_000);
  flush();
  fireEvent.keyDown(view.slider, { key: "PageDown" });
  expect(view.onJump).toHaveBeenLastCalledWith(750_000);
  flush();
  fireEvent.keyDown(view.slider, { key: "End" });
  expect(view.onJump).toHaveBeenLastCalledWith(999_990);
  flush();
  fireEvent.keyDown(view.slider, { key: "Home" });
  expect(view.onJump).toHaveBeenLastCalledWith(0);
  const ignored = fireEvent.keyDown(view.slider, { key: "ArrowDown", ctrlKey: true });
  expect(ignored).toBe(true);
  expect(view.onHold).not.toHaveBeenCalled();
});

it("drags once per frame with demand held, and releases demand on pointer up", () => {
  const view = mount();
  fireEvent.pointerDown(view.slider, { button: 0, clientY: 4 + 300, pointerId: 3 });
  fireEvent.pointerMove(view.slider, { clientY: 4 + 450, pointerId: 3 });
  fireEvent.pointerMove(view.slider, { clientY: 4 + 480, pointerId: 3 });
  // Three pointer events, one frame: only the latest target is applied.
  expect(view.onHold).toHaveBeenCalledTimes(3);
  expect(view.onJump).not.toHaveBeenCalled();
  runFrames();
  expect(view.onJump).toHaveBeenCalledTimes(1);
  expect(view.onJump).toHaveBeenLastCalledWith(800_000);
  expect(screen.getByTestId("gallery-timeline-label").textContent).toBe("December 2023");
  expect(view.slider.hasAttribute("data-gallery-timeline-dragging")).toBe(true);
  fireEvent.pointerUp(view.slider, { pointerId: 3 });
  flush();
  expect(view.onRelease).toHaveBeenCalledTimes(1);
  expect(view.slider.hasAttribute("data-gallery-timeline-dragging")).toBe(false);
  // Hovering only previews the month under the pointer.
  fireEvent.pointerMove(view.slider, { clientY: 4 + 60 });
  flush();
  expect(screen.getByTestId("gallery-timeline-label").textContent).toBe("March 2024");
  runFrames();
  expect(view.onJump).toHaveBeenCalledTimes(1);
});

it("takes keyboard focus on a mouse press even though pointerdown is default-prevented", () => {
  const view = mount();
  const other = document.createElement("button");
  document.body.append(other);
  other.focus();
  const focus = vi.spyOn(view.slider, "focus");
  expect(fireEvent.pointerDown(view.slider, { button: 0, clientY: 4 + 300, pointerId: 3 })).toBe(
    false
  );
  expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  expect(document.activeElement).toBe(view.slider);
  fireEvent.pointerUp(view.slider, { pointerId: 3 });
  // Keys that follow the click move the timeline.
  fireEvent.keyDown(document.activeElement as Element, { key: "ArrowDown" });
  expect(view.onJump).toHaveBeenLastCalledWith(500_000);
  // A press the rail ignores does not steal focus.
  other.focus();
  fireEvent.pointerDown(view.slider, { button: 2, pointerId: 4 });
  expect(document.activeElement).toBe(other);
  other.remove();
});

it("reports and steps from its own target when the scroll clamps near the end", () => {
  // The last row that can reach the top is in December 2023.
  const view = mount(999_000);
  view.slider.focus();
  fireEvent.keyDown(view.slider, { key: "End" });
  flush();
  runFrames();
  expect(view.onJump).toHaveBeenLastCalledWith(999_990);
  expect(view.slider.getAttribute("aria-valuetext")).toBe("No date");
  fireEvent.keyDown(view.slider, { key: "ArrowUp" });
  flush();
  runFrames();
  expect(view.onJump).toHaveBeenLastCalledWith(750_000);
  expect(view.slider.getAttribute("aria-valuetext")).toBe("December 2023");
  // Scrolling elsewhere hands the label back to the first visible item.
  view.setFirst(100);
  flush();
  expect(view.slider.getAttribute("aria-valuetext")).toBe("March 2024");
  fireEvent.keyDown(view.slider, { key: "ArrowDown" });
  expect(view.onJump).toHaveBeenLastCalledWith(500_000);
});
