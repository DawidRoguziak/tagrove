import { cleanup, renderHook } from "@solidjs/testing-library";
import { createEffect, createSignal, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createVirtualGrid } from "../createVirtualGrid";
import { MAX_GRID_HEIGHT } from "../gridGeometry";
const callbacks = new Set<(entries: ResizeObserverEntry[]) => void>();
class Observer {
  constructor(readonly callback: (entries: ResizeObserverEntry[]) => void) {}
  observe() {
    callbacks.add(this.callback);
  }
  unobserve() {
    callbacks.delete(this.callback);
  }
  disconnect() {
    callbacks.delete(this.callback);
  }
}
beforeEach(() => vi.stubGlobal("ResizeObserver", Observer));
afterEach(() => {
  cleanup();
  document.body.replaceChildren();
  callbacks.clear();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
const FOOTER = 136;
function mount(count = 10000, delayed = false, hasMore = false) {
  const scroller = document.createElement("div"),
    gallery = document.createElement("section"),
    grid = document.createElement("div"),
    input = document.createElement("input");
  scroller.append(gallery, input);
  gallery.append(grid);
  document.body.append(scroller);
  let width = 988;
  let top = 0;
  let height = 600;
  let margin = 80;
  Object.defineProperties(scroller, {
    offsetWidth: { get: () => width },
    clientWidth: { get: () => width },
    offsetHeight: { get: () => height },
    clientHeight: { get: () => height },
    scrollHeight: { get: () => Number.parseFloat(grid.style.height || "0") + margin + FOOTER },
    scrollTop: {
      get: () => top,
      set: (value: number) => {
        // WebKit rounds and clamps physical offsets.
        top = Math.round(Math.max(0, Math.min(value, scroller.scrollHeight - height)));
      }
    }
  });
  Object.defineProperty(grid, "clientWidth", { get: () => width });
  vi.spyOn(scroller, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, 0, width, height)
  );
  vi.spyOn(grid, "getBoundingClientRect").mockImplementation(
    () => new DOMRect(0, margin - scroller.scrollTop, width, 2000000)
  );
  const range = vi.fn();
  const reachEnd = vi.fn();
  const view = renderHook(() => {
    const [ready, setReady] = createSignal(!delayed);
    const [tile, setTile] = createSignal(188);
    const [loading, setLoading] = createSignal(false);
    const [total, setTotal] = createSignal(count);
    const [reset, setReset] = createSignal(0);
    const model = createVirtualGrid({
      get assetCount() {
        return total();
      },
      get tileSize() {
        return tile();
      },
      hasMore,
      get isLoading() {
        return loading();
      },
      gallery: () => (ready() ? gallery : undefined),
      grid: () => (ready() ? grid : undefined),
      scroller: () => (ready() ? scroller : undefined),
      onReachEnd: reachEnd,
      onRange: range,
      get rangeResetKey() {
        return reset();
      }
    });
    return { model, setReady, setLoading, setTotal, setReset, setTile };
  });
  flush();
  const observe = () => {
    for (const callback of callbacks) callback([]);
    flush();
  };
  return {
    ...view,
    scroller,
    gallery,
    grid,
    input,
    range,
    reachEnd,
    resizeHeight(next: number) {
      height = next;
      observe();
    },
    resize(next: number) {
      width = next;
      observe();
    },
    moveMargin(next: number) {
      margin = next;
      observe();
    },
    scroll(next: number) {
      scroller.scrollTop = next;
      scroller.dispatchEvent(new Event("scroll"));
      flush();
    },
    key(key: string, target: Element = scroller) {
      const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      target.dispatchEvent(event);
      flush();
      return event;
    },
    lastRange: () => range.mock.lastCall?.[0]
  };
}

describe("custom virtual grid", () => {
  it.each([
    10000, 26000
  ])("bounds every initial and remount range with %i slots and null parent refs", (count) => {
    for (let pass = 0; pass < 2; pass++) {
      const view = mount(count, true),
        model = view.result.model;
      expect(model.range.endIndex).toBe(-1);
      view.result.setReady(true);
      flush();
      // The first width measurement must not scroll the margin above the grid away.
      expect(view.scroller.scrollTop).toBe(0);
      expect(model.columnCount).toBe(5);
      expect(model.range.endIndex).toBeGreaterThan(0);
      for (const [range] of view.range.mock.calls)
        if (range) expect(range.endIndex - range.startIndex + 1).toBeLessThan(70);
      view.scroll(20080);
      expect(model.range.startIndex).toBeGreaterThan(450);
      expect(model.range.endIndex - model.range.startIndex + 1).toBeLessThan(70);
      view.cleanup();
      expect(callbacks.size).toBe(0);
      expect(view.lastRange()).toBeNull();
    }
  });

  it("publishes demand only when the row range changes", () => {
    vi.useFakeTimers();
    const view = mount();
    // Mid-row, so the bottom edge does not sit on a row boundary.
    view.scroll(20180);
    vi.advanceTimersByTime(150);
    flush();
    view.range.mockClear();
    for (let step = 1; step < 20; step++) {
      view.scroll(20180 + step * 5);
      vi.advanceTimersByTime(20);
    }
    expect(view.range).not.toHaveBeenCalled();
    view.scroll(20280);
    expect(view.range).toHaveBeenCalledOnce();
    expect(view.lastRange()).toMatchObject({ visibleStartIndex: 505, paused: false });
  });

  it("keeps layout getters quiet while only the rendered rows change", () => {
    const view = mount();
    let runs = 0;
    renderHook(() =>
      createEffect(
        () => [
          view.result.model.columnCount,
          view.result.model.tilePixelSize,
          view.result.model.gridOffset,
          view.result.model.poolSize,
          view.result.model.physicalHeight
        ],
        () => {
          runs++;
        }
      )
    );
    flush();
    // The selection handlers cancel an active drag when these change.
    for (let top = 1000; top <= 20000; top += 700) view.scroll(top);
    expect(view.result.model.firstRow).toBeGreaterThan(80);
    expect(runs).toBe(1);
    view.resize(588);
    expect(runs).toBe(2);
  });

  it("anchors resizing on the first visible global index and respects the external margin", () => {
    const view = mount();
    view.scroll(2080);
    view.resize(588);
    expect(view.result.model.columnCount).toBe(3);
    expect(view.scroller.scrollTop).toBe(3280);
    expect(view.lastRange().visibleStartIndex).toBe(48);
    view.result.setTile(340);
    flush();
    // Index 48 is row 48 at 588px with a single 340px-minimum column.
    expect(view.result.model.columnCount).toBe(1);
    expect(view.lastRange().visibleStartIndex).toBe(48);
    view.resizeHeight(450);
    expect(view.lastRange().visibleStartIndex).toBe(48);
  });

  it("keeps the margin above the grid visible when the first measurement changes columns", () => {
    const view = mount(10_000, true);
    view.result.setTile(140);
    flush();
    view.result.setReady(true);
    flush();
    view.resize(588);
    expect(view.scroller.scrollTop).toBe(0);
    expect(view.lastRange()).toMatchObject({ startIndex: 0, visibleStartIndex: 0 });
  });

  it("keeps the scroll position when only the margin above the grid changes", () => {
    const view = mount();
    view.scroll(2080);
    expect(view.lastRange().visibleStartIndex).toBe(50);
    view.moveMargin(128);
    expect(view.scroller.scrollTop).toBe(2080);
    expect(view.result.model.gridOffset).toBe(128);
    // The grid moved down 48px under an unchanged scroll offset.
    expect(view.lastRange().visibleStartIndex).toBe(45);
  });

  it("retains a partial last row and publishes no slots when empty", () => {
    const view = mount(12);
    expect(view.result.model.range).toEqual({
      startIndex: 0,
      endIndex: 11,
      visibleStartIndex: 0,
      visibleEndIndex: 11
    });
    expect(view.result.model.physicalHeight).toBe(588);
    expect(view.grid.style.height).toBe("588px");
    view.result.setTotal(0);
    flush();
    expect(view.result.model.range.endIndex).toBe(-1);
  });

  it("never intercepts wheel input", () => {
    for (const count of [10_000, 1_000_000]) {
      const view = mount(count);
      const wheel = new WheelEvent("wheel", { deltaY: 100, bubbles: true, cancelable: true });
      view.gallery.dispatchEvent(wheel);
      expect(wheel.defaultPrevented).toBe(false);
      view.cleanup();
    }
  });
});

describe("windowed scrolling above the physical height limit", () => {
  it("reaches the millionth slot through a bounded native window", () => {
    const view = mount(1_000_000);
    const model = view.result.model;
    expect(model.windowed).toBe(true);
    expect(model.physicalHeight).toBe(MAX_GRID_HEIGHT);
    expect(view.grid.style.height).toBe(`${MAX_GRID_HEIGHT}px`);
    expect(view.scroller.dataset.galleryWindowed).toBe("");
    expect(view.key("End").defaultPrevented).toBe(true);
    expect(model.scrollShift).toBe(model.maxBase);
    expect(view.lastRange()).toMatchObject({ endIndex: 999_999, visibleEndIndex: 999_999 });
    expect(view.scroller.scrollTop).toBeLessThanOrEqual(MAX_GRID_HEIGHT + 80 + FOOTER - 600);
    expect(view.key("Home").defaultPrevented).toBe(true);
    expect(model.scrollShift).toBe(0);
    expect(view.scroller.scrollTop).toBe(0);
    expect(view.lastRange()).toMatchObject({ startIndex: 0, visibleStartIndex: 0 });
    // Focus dropped to the body by a recycled row still reaches the whole result.
    expect(view.key("End", document.body).defaultPrevented).toBe(true);
    expect(view.lastRange()).toMatchObject({ endIndex: 999_999 });
    view.key("Home");
    // Text controls and panels outside the gallery keep their own Home/End.
    expect(view.key("End", view.input).defaultPrevented).toBe(false);
    view.cleanup();
    expect(view.scroller.dataset.galleryWindowed).toBeUndefined();
    expect(callbacks.size).toBe(0);
  });

  it("moves the window just before native scrolling reaches its edge, keeping the logical row", () => {
    const view = mount(1_000_000);
    const model = view.result.model;
    view.scroll(8_000_080);
    expect(model.scrollShift).toBe(0);
    const middle = view.lastRange().visibleStartIndex;
    expect(middle).toBe(200_000);
    // Within two viewports of the physical end: the base moves at once.
    view.scroll(80 + MAX_GRID_HEIGHT - 1700);
    const logical = MAX_GRID_HEIGHT - 1700;
    expect(model.scrollShift).toBeGreaterThan(0);
    expect(view.scroller.scrollTop).toBe(80 + logical - model.scrollShift);
    expect(model.offset).toBe(logical);
    expect(view.lastRange().visibleStartIndex).toBe(Math.floor(logical / 200) * 5);
  });

  it("recentres the window after 150ms without motion in its outer quarter", () => {
    vi.useFakeTimers();
    const view = mount(1_000_000);
    const model = view.result.model;
    view.scroll(80 + 13_000_000);
    expect(model.scrollShift).toBe(0);
    vi.advanceTimersByTime(149);
    flush();
    expect(model.scrollShift).toBe(0);
    vi.advanceTimersByTime(1);
    flush();
    expect(model.scrollShift).toBeGreaterThan(0);
    expect(model.offset).toBe(13_000_000);
    expect(view.scroller.scrollTop - 80).toBe(13_000_000 - model.scrollShift);
    expect(view.lastRange().visibleStartIndex).toBe(325_000);
    view.cleanup();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps subpixel edge-scroll steps logical despite rounded physical offsets", () => {
    const view = mount(1_000_000);
    const model = view.result.model;
    view.scroll(5_000_080);
    for (let i = 0; i < 10; i++) model.scrollBy(0.4);
    flush();
    expect(model.offset).toBeCloseTo(5_000_004);
    expect(view.scroller.scrollTop).toBe(5_000_084);
  });

  it("scrolls a global index to the top and restores native mode when the result shrinks", () => {
    const view = mount(1_000_000);
    const model = view.result.model;
    model.scrollToIndex(777_777);
    flush();
    expect(view.lastRange().visibleStartIndex).toBe(777_775);
    expect(model.scrollShift).toBeGreaterThan(0);
    view.result.setTotal(100);
    flush();
    expect(model.windowed).toBe(false);
    expect(model.scrollShift).toBe(0);
    expect(view.scroller.dataset.galleryWindowed).toBeUndefined();
    expect(view.scroller.style.paddingRight).toBe("");
    expect(view.grid.style.height).toBe(`${20 * 200 - 12}px`);
    expect(view.lastRange().endIndex).toBe(99);
    // Native grids leave Home/End to the browser.
    expect(view.key("End").defaultPrevented).toBe(false);
  });
});

it("pauses on a half-viewport jump and resumes the same range after 150ms without movement", () => {
  vi.useFakeTimers();
  const view = mount();
  view.scroll(380);
  expect(view.result.model.paused).toBe(true);
  const latest = view.lastRange();
  vi.advanceTimersByTime(149);
  flush();
  expect(view.result.model.paused).toBe(true);
  // Duplicate scroll notifications must not extend the pause.
  view.scroll(380);
  vi.advanceTimersByTime(1);
  flush();
  expect(view.range).toHaveBeenLastCalledWith({ ...latest, paused: false });
  view.cleanup();
});

it("keeps slow scrolling active and counts distance across direction changes in a rolling 100ms", () => {
  vi.useFakeTimers();
  const view = mount();
  for (let top = 100; top <= 500; top += 100) {
    view.scroll(top);
    vi.advanceTimersByTime(101);
    flush();
    expect(view.result.model.paused).toBe(false);
  }
  view.scroll(700);
  vi.advanceTimersByTime(30);
  view.scroll(500);
  vi.advanceTimersByTime(30);
  view.scroll(700);
  expect(view.result.model.paused).toBe(true);
  vi.advanceTimersByTime(100);
  view.scroll(690);
  vi.advanceTimersByTime(149);
  flush();
  expect(view.result.model.paused).toBe(true);
  vi.advanceTimersByTime(1);
  flush();
  expect(view.result.model.paused).toBe(false);
  view.cleanup();
});

it("excludes geometry anchors from motion and clears paused demand and timers on disposal", () => {
  vi.useFakeTimers();
  const view = mount(1_000_000);
  view.scroll(2_000_000);
  vi.advanceTimersByTime(150);
  flush();
  view.resize(588);
  view.result.setTile(340);
  flush();
  view.resizeHeight(450);
  expect(view.result.model.paused).toBe(false);
  view.result.model.scrollBy(225);
  flush();
  expect(view.result.model.paused).toBe(true);
  view.cleanup();
  expect(view.range).toHaveBeenLastCalledWith(null);
  view.range.mockClear();
  vi.advanceTimersByTime(1000);
  flush();
  expect(view.range).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("does not reach-end load during a paused jump, including after loading finishes", () => {
  vi.useFakeTimers();
  const view = mount(10000, false, true);
  view.scroll(Number.MAX_SAFE_INTEGER);
  expect(view.result.model.paused).toBe(true);
  view.result.setLoading(true);
  flush();
  view.result.setLoading(false);
  flush();
  expect(view.reachEnd).not.toHaveBeenCalled();
  vi.advanceTimersByTime(150);
  flush();
  expect(view.reachEnd).toHaveBeenCalledOnce();
  view.cleanup();
});

it("keeps retried ranges and the published range object stable across loading updates", () => {
  const view = mount();
  view.scroll(20080);
  const published = view.result.model.range;
  view.result.setLoading(true);
  flush();
  expect(view.result.model.range).toBe(published);
  view.range.mockClear();
  view.result.setReset(1);
  flush();
  expect(view.range).toHaveBeenCalledOnce();
  expect(view.lastRange()).toMatchObject({ visibleStartIndex: 500 });
});
