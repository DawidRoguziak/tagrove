import { cleanup, fireEvent, renderHook } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { settle } from "../../../test/settle";
import {
  createLightboxActivity,
  createLightboxSidebar,
  createGroupClipboard
} from "../createLightboxPresentation";
import { deferred } from "./fixtures";
const originalMatch = window.matchMedia;
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.matchMedia = originalMatch;
});
function activity() {
  return renderHook(() => {
    const [active, setActive] = createSignal(true);
    const model = createLightboxActivity(active);
    return { model, setActive };
  });
}
describe("lightbox presentation owners", () => {
  it("hides after three seconds and only movement restarts the timer", () => {
    const {
      result: { model }
    } = activity();
    settle(() => vi.advanceTimersByTime(2999));
    expect(model.visible()).toBe(true);
    const move = (x: number) =>
      settle(() => fireEvent(window, new MouseEvent("pointermove", { clientX: x, clientY: 20 })));
    move(10);
    settle(() => vi.advanceTimersByTime(2999));
    expect(model.visible()).toBe(true);
    move(10);
    settle(() => vi.advanceTimersByTime(1));
    expect(model.visible()).toBe(false);
    move(11);
    expect(model.visible()).toBe(true);
  });
  it.each([
    "pointerdown",
    "wheel",
    "scroll",
    "keydown"
  ])("reveals without consuming %s and releases timers on close", (name) => {
    const {
      result: { model, setActive }
    } = activity();
    settle(() => vi.advanceTimersByTime(3000));
    expect(model.visible()).toBe(false);
    const event = new Event(name, { bubbles: true, cancelable: true });
    settle(() => fireEvent(window, event));
    expect(model.visible()).toBe(true);
    expect(event.defaultPrevented).toBe(false);
    settle(() => setActive(false));
    expect(vi.getTimerCount()).toBe(0);
    settle(model.reveal);
    expect(vi.getTimerCount()).toBe(0);
  });
  it("does not hold controls visible for focus alone", () => {
    const {
      result: { model }
    } = activity();
    const button = document.createElement("button");
    document.body.append(button);
    button.focus();
    settle(() => vi.advanceTimersByTime(3000));
    expect(model.visible()).toBe(false);
    button.remove();
  });
  it("keeps touch controls visible", () => {
    window.matchMedia = vi.fn(() => ({
      matches: true,
      media: "",
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: () => true
    }));
    const {
      result: { model }
    } = activity();
    settle(() => vi.advanceTimersByTime(10000));
    expect(model.visible()).toBe(true);
  });
  it("follows breakpoints until explicit choice, retains occupied space and resets on close", () => {
    let narrow = false;
    const events = new EventTarget();
    window.matchMedia = vi.fn((query) => ({
      get matches() {
        return query.includes("767") && narrow;
      },
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: events.addEventListener.bind(events),
      removeEventListener: events.removeEventListener.bind(events),
      dispatchEvent: events.dispatchEvent.bind(events)
    }));
    const { result } = renderHook(() => {
      const [active, setActive] = createSignal(true),
        [locked, setLocked] = createSignal(false);
      return { model: createLightboxSidebar(active, locked), setActive, setLocked };
    });
    const { model } = result;
    expect(model.open()).toBe(true);
    settle(() => {
      narrow = true;
      events.dispatchEvent(new Event("change"));
    });
    expect(model.open()).toBe(false);
    expect(model.occupied()).toBe(true);
    settle(() => vi.advanceTimersByTime(200));
    expect(model.occupied()).toBe(false);
    settle(model.show);
    settle(() => {
      narrow = false;
      events.dispatchEvent(new Event("change"));
      narrow = true;
      events.dispatchEvent(new Event("change"));
    });
    expect(model.open()).toBe(true);
    settle(() => result.setLocked(true));
    settle(model.hide);
    expect(model.open()).toBe(true);
    settle(() => {
      result.setLocked(false);
      result.setActive(false);
    });
    settle(() => result.setActive(true));
    expect(model.open()).toBe(false);
  });
  it("copies trimmed names, resets after 1600ms and ignores disposed replies", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: write }
    });
    const view = renderHook(() =>
      createGroupClipboard(
        () => " group ",
        () => 1
      )
    );
    await settle(view.result.copy);
    expect(write).toHaveBeenCalledWith("group");
    expect(view.result.confirmed()).toBe(true);
    settle(() => vi.advanceTimersByTime(1600));
    expect(view.result.confirmed()).toBe(false);
    const pending = deferred<void>();
    write.mockReturnValue(pending.promise);
    const copying = view.result.copy();
    view.cleanup();
    pending.resolve();
    expect(await copying).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });
});
