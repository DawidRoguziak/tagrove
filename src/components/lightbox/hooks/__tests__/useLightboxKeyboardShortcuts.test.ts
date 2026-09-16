import { act, cleanup, renderHook } from "@testing-library/react";
import { StrictMode, useEffect } from "react";
import { flushSync } from "react-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../../types";
import { useLightboxActivity } from "../useLightboxActivity";
import { useLightboxKeyboardShortcuts } from "../useLightboxKeyboardShortcuts";

function createOptions() {
  const selected: SelectedAsset = {
    id: 1, file_name: "1.png", preview_path: null, kind: "image", modified_at: 1,
    width: 100, height: 100, duration_ms: null, thumb_path: null, is_favorite: false,
    media_group_key: null, media_group_order: null, path: "/fixtures/1.png",
    size_bytes: 100, tags: []
  };
  return {
    enabled: true,
    selected,
    isFullscreen: false,
    onNavigatePrevious: vi.fn(),
    onNavigateNext: vi.fn(),
    onToggleFullscreen: vi.fn(async () => {}),
    onResetZoom: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn()
  };
}

function press(key: string, target: EventTarget = window) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  act(() => { target.dispatchEvent(event); });
  return event;
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("useLightboxKeyboardShortcuts", () => {
  it.each(["ArrowLeft", "ArrowRight"])("handles the first %s after idle even when an earlier listener commits a render", (key) => {
    vi.useFakeTimers();
    const navigate = vi.fn();
    const options = createOptions();
    const { result } = renderHook(() => {
      const activity = useLightboxActivity(true);
      useEffect(() => {
        const reveal = () => flushSync(() => activity.reveal());
        window.addEventListener("keydown", reveal, true);
        return () => window.removeEventListener("keydown", reveal, true);
      }, [activity.reveal]);
      useLightboxKeyboardShortcuts({
        ...options,
        onNavigatePrevious: () => navigate(activity.visible),
        onNavigateNext: () => navigate(activity.visible)
      });
      return activity;
    });

    for (let count = 1; count <= 3; count += 1) {
      act(() => vi.advanceTimersByTime(3001));
      expect(result.current.visible).toBe(false);
      expect(press(key).defaultPrevented).toBe(true);
      expect(result.current.visible).toBe(true);
      expect(navigate).toHaveBeenCalledTimes(count);
      expect(navigate).toHaveBeenLastCalledWith(true);
    }
  });

  it("uses current callbacks, media kind and fullscreen without replacing the listener", () => {
    const initial = createOptions();
    const { rerender } = renderHook(useLightboxKeyboardShortcuts, { initialProps: initial });
    const add = vi.spyOn(window, "addEventListener");
    const remove = vi.spyOn(window, "removeEventListener");
    const current = createOptions();
    current.selected.id = 2;
    rerender(current);
    for (const key of ["ArrowLeft", "ArrowRight", "f", "0", "+", "-"]) {
      expect(press(key).defaultPrevented).toBe(true);
    }
    for (const callback of ["onNavigatePrevious", "onNavigateNext", "onToggleFullscreen", "onResetZoom", "onZoomIn", "onZoomOut"] as const) {
      expect(initial[callback]).not.toHaveBeenCalled();
      expect(current[callback]).toHaveBeenCalledTimes(1);
    }

    rerender({ ...current, selected: { ...current.selected, kind: "video" }, isFullscreen: true });
    expect(press("0").defaultPrevented).toBe(false);
    expect(press("Escape").defaultPrevented).toBe(true);
    expect(current.onToggleFullscreen).toHaveBeenCalledTimes(2);
    rerender({ ...current, selected: { ...current.selected, kind: "video" }, isFullscreen: false });
    expect(press("Escape").defaultPrevented).toBe(false);
    expect(add.mock.calls.filter(([type]) => type === "keydown")).toHaveLength(0);
    expect(remove.mock.calls.filter(([type]) => type === "keydown")).toHaveLength(0);
  });

  it("consumes fullscreen Escape from form focus and ignores key repeat", () => {
    const options = createOptions();
    renderHook(() => useLightboxKeyboardShortcuts({ ...options,
      selected: { ...options.selected, kind: "video" }, isFullscreen: true }));
    const input = document.createElement("input");
    document.body.append(input);
    try {
      expect(press("Escape", input).defaultPrevented).toBe(true);
      const repeat = new KeyboardEvent("keydown", { key: "Escape", repeat: true, bubbles: true, cancelable: true });
      input.dispatchEvent(repeat);
      expect(repeat.defaultPrevented).toBe(true);
      expect(options.onToggleFullscreen).toHaveBeenCalledOnce();
    } finally { input.remove(); }
  });

  it("cleans up when disabled, closed or unmounted and reopens once in Strict Mode", () => {
    const options = createOptions();
    const initialProps: Parameters<typeof useLightboxKeyboardShortcuts>[0] = options;
    const { rerender, unmount } = renderHook(
      (props: Parameters<typeof useLightboxKeyboardShortcuts>[0]) => useLightboxKeyboardShortcuts(props),
      { initialProps, wrapper: StrictMode }
    );
    expect(press("ArrowRight").defaultPrevented).toBe(true);
    expect(options.onNavigateNext).toHaveBeenCalledTimes(1);
    rerender({ ...options, enabled: false });
    expect(press("ArrowRight").defaultPrevented).toBe(false);
    rerender(options);
    press("ArrowRight");
    expect(options.onNavigateNext).toHaveBeenCalledTimes(2);
    rerender({ ...options, selected: null });
    expect(press("ArrowRight").defaultPrevented).toBe(false);
    rerender(options);
    press("ArrowRight");
    expect(options.onNavigateNext).toHaveBeenCalledTimes(3);
    unmount();
    expect(press("ArrowRight").defaultPrevented).toBe(false);
    const reopened = renderHook(() => useLightboxKeyboardShortcuts(options), { wrapper: StrictMode });
    press("ArrowRight");
    expect(options.onNavigateNext).toHaveBeenCalledTimes(4);
    reopened.unmount();
    expect(press("ArrowRight").defaultPrevented).toBe(false);
  });
});
