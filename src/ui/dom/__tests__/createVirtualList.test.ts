import { cleanup, renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createVirtualList } from "../createVirtualList";

const observed = new Set<Element>();
const callbacks = new Set<ResizeObserverCallback>();
class Observer {
  constructor(readonly callback: ResizeObserverCallback) {
    callbacks.add(callback);
  }
  observe(element: Element) {
    observed.add(element);
  }
  unobserve(element: Element) {
    observed.delete(element);
  }
  disconnect() {
    callbacks.delete(this.callback);
  }
}
beforeEach(() => vi.stubGlobal("ResizeObserver", Observer));
afterEach(() => {
  cleanup();
  observed.clear();
  callbacks.clear();
  vi.unstubAllGlobals();
});
function mount(count = 1000, estimate = 100) {
  const scroller = document.createElement("div");
  document.body.append(scroller);
  let top = 0;
  Object.defineProperties(scroller, {
    clientHeight: { value: 500 },
    scrollTop: {
      get: () => top,
      set: (value: number) => {
        top = Math.max(0, value);
      }
    }
  });
  const view = renderHook(() => {
    const [total, setTotal] = createSignal(count);
    const [size, setSize] = createSignal(estimate);
    const [enabled, setEnabled] = createSignal(true);
    const list = createVirtualList({
      scroller: () => scroller,
      count: total,
      estimateSize: size,
      overscan: 2,
      initialViewport: 720,
      enabled
    });
    return { list, setTotal, setSize, setEnabled };
  });
  flush();
  const scroll = (value: number) => {
    scroller.scrollTop = value;
    scroller.dispatchEvent(new Event("scroll"));
    flush();
  };
  return { ...view.result, scroller, scroll, cleanup: view.cleanup };
}
const indexes = (items: { index: number }[]) => items.map((item) => item.index);

describe("createVirtualList", () => {
  it("windows a fixed-size list with overscan and keeps item identity while scrolling", () => {
    const view = mount();
    expect(indexes(view.list.items())).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(view.list.totalSize()).toBe(100_000);
    const retained = view.list.items()[5];
    view.scroll(250);
    expect(indexes(view.list.items())).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(view.list.items()[5]).toBe(retained);
    view.scroll(99_500);
    expect(indexes(view.list.items())).toEqual([993, 994, 995, 996, 997, 998, 999]);
  });

  it("re-lays out on estimate and count changes and renders nothing when empty", () => {
    const view = mount();
    view.setSize(250);
    flush();
    expect(view.list.totalSize()).toBe(250_000);
    expect(view.list.items()[1]).toMatchObject({ index: 1, start: 250, size: 250 });
    view.setTotal(0);
    flush();
    expect(view.list.items()).toEqual([]);
    expect(view.list.totalSize()).toBe(0);
  });

  it("uses measured heights, observes elements and keeps content above the viewport stable", () => {
    const view = mount(20, 360);
    const element = document.createElement("div");
    view.scroller.append(element);
    Object.defineProperty(element, "offsetHeight", { configurable: true, value: 720 });
    view.list.measure(element, 0);
    flush();
    expect(observed.has(element)).toBe(true);
    expect(view.list.items()[1]).toMatchObject({ start: 720, size: 360 });
    view.scroll(1000);
    Object.defineProperty(element, "offsetHeight", { configurable: true, value: 820 });
    for (const callback of callbacks)
      callback([{ target: element } as unknown as ResizeObserverEntry], {} as ResizeObserver);
    flush();
    expect(view.scroller.scrollTop).toBe(1100);
    expect(view.list.totalSize()).toBe(820 + 19 * 360);
    element.remove();
    for (const callback of callbacks)
      callback([{ target: element } as unknown as ResizeObserverEntry], {} as ResizeObserver);
    expect(observed.has(element)).toBe(false);
  });

  it("scrolls an index into view only when needed", () => {
    const view = mount();
    view.list.scrollToIndex(3);
    expect(view.scroller.scrollTop).toBe(0);
    view.list.scrollToIndex(20);
    flush();
    expect(view.scroller.scrollTop).toBe(1600);
    expect(indexes(view.list.items())).toContain(20);
    view.list.scrollToIndex(2);
    expect(view.scroller.scrollTop).toBe(200);
    view.list.scrollToIndex(50, "start");
    expect(view.scroller.scrollTop).toBe(5000);
  });

  it("stops observing while disabled and on disposal", () => {
    const view = mount();
    expect(callbacks.size).toBe(2);
    view.setEnabled(false);
    flush();
    expect(callbacks.size).toBe(0);
    view.scroll(5000);
    expect(indexes(view.list.items())[0]).toBe(0);
    view.setEnabled(true);
    flush();
    expect(indexes(view.list.items())).toContain(50);
    view.cleanup();
    expect(callbacks.size).toBe(0);
  });
});
