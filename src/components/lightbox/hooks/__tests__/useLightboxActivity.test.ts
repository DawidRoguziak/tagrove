import { act, cleanup, fireEvent, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useLightboxActivity } from "../useLightboxActivity";

describe("lightbox activity", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("hides after three seconds and resets on actual movement anywhere in the app", () => {
    const { result } = renderHook(() => useLightboxActivity(true));
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    fireEvent.mouseMove(document.body, { clientX: 10, clientY: 20 });
    act(() => vi.advanceTimersByTime(2999));
    expect(result.current.visible).toBe(true);
    fireEvent.mouseMove(document.body, { clientX: 10, clientY: 20 });
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.visible).toBe(false);
    fireEvent.mouseMove(window, { clientX: 11, clientY: 20 });
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
