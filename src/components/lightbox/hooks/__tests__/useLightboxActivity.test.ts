import { act, cleanup, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useLightboxActivity } from "../useLightboxActivity";

describe("lightbox activity", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("PointerEvent", MouseEvent);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("hides after three seconds and resets on actual movement anywhere in the app", () => {
    const { result } = renderHook(() => useLightboxActivity(true));
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    fireEvent.pointerMove(document.body, { clientX: 10, clientY: 20 });
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    fireEvent.pointerMove(document.body, { clientX: 10, clientY: 20 });
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.visible).toBe(false);
    fireEvent.pointerMove(window, { clientX: 11, clientY: 20 });
    expect(result.current.visible).toBe(true);
  });

  it("accepts native activity, keyboard discovery, and clears pending work on close", () => {
    const { result, rerender } = renderHook(({ active }) => useLightboxActivity(active), {
      initialProps: { active: true }
    });
    act(() => vi.advanceTimersByTime(3000));
    act(() => result.current.reveal());
    expect(result.current.visible).toBe(true);
    act(() => vi.advanceTimersByTime(3000));
    fireEvent.keyDown(window, { key: "Tab" });
    expect(result.current.visible).toBe(true);
    rerender({ active: false });
    expect(vi.getTimerCount()).toBe(0);
    act(() => result.current.reveal());
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each(["pointerdown", "wheel", "scroll", "keydown"])("resets the timer on %s without consuming the action", (type) => {
    const { result, unmount } = renderHook(() => useLightboxActivity(true));
    const target = document.createElement("div");
    document.body.append(target);
    const action = vi.fn();
    target.addEventListener(type, action);
    act(() => vi.advanceTimersByTime(3000));
    expect(result.current.visible).toBe(false);
    const event = new Event(type, { bubbles: true, cancelable: true });
    fireEvent(target, event);
    expect(result.current.visible).toBe(true);
    expect(action).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(false);
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    fireEvent(target, new Event(type, { bubbles: true }));
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.visible).toBe(false);
    unmount();
    fireEvent(target, new Event(type, { bubbles: true }));
    expect(vi.getTimerCount()).toBe(0);
    target.remove();
  });

  it("does not keep controls visible just because a control has focus", () => {
    const { result } = renderHook(() => useLightboxActivity(true));
    const button = document.createElement("button");
    document.body.append(button);
    button.focus();
    act(() => vi.advanceTimersByTime(3000));
    expect(result.current.visible).toBe(false);
    expect(button).toHaveFocus();
    button.remove();
  });

  it("keeps the trigger visible on touch devices", () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn(() => ({
        matches: true,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn()
      }))
    );
    const { result } = renderHook(() => useLightboxActivity(true));
    act(() => vi.advanceTimersByTime(10000));
    expect(result.current.visible).toBe(true);
    vi.unstubAllGlobals();
  });
});
