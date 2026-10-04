import { cleanup, renderHook } from "@solidjs/testing-library";
import { createSignal, flush, onSettled } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { createImageControls } from "../image/createImageControls";
import { createLightboxActivity } from "../createLightboxPresentation";
import { asset } from "./fixtures";

function press(key: string, target: EventTarget = window, repeat = false) {
  const event = new KeyboardEvent("keydown", { key, repeat, bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  flush();
  return event;
}

function options() {
  return {
    selected: () => asset(1), active: () => true, keyboardEnabled: () => true,
    close: vi.fn(), previous: vi.fn(), next: vi.fn(), enterFullscreen: vi.fn()
  };
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it.each(["ArrowLeft", "ArrowRight"])("handles the first %s after idle and a synchronous activity update", key => {
  vi.useFakeTimers();
  const navigate = vi.fn();
  const { result } = renderHook(() => {
    const activity = createLightboxActivity(() => true);
    // Register ahead of the controls, as an earlier capture listener can flush updates.
    onSettled(() => {
      const reveal = () => { activity.reveal(); flush(); };
      window.addEventListener("keydown", reveal, true);
      return () => window.removeEventListener("keydown", reveal, true);
    });
    createImageControls({ ...options(), previous: () => navigate(activity.visible()), next: () => navigate(activity.visible()) });
    return activity;
  });
  flush();
  for (let count = 1; count <= 3; count++) {
    vi.advanceTimersByTime(3001);
    flush();
    expect(result.visible()).toBe(false);
    expect(press(key).defaultPrevented).toBe(true);
    expect(result.visible()).toBe(true);
    expect(navigate).toHaveBeenCalledTimes(count);
    expect(navigate).toHaveBeenLastCalledWith(true);
  }
});

it("reads current callbacks and media kind without replacing the keyboard listener", () => {
  const initial = options();
  const current = options();
  const { result } = renderHook(() => {
    const [value, setValue] = createSignal(() => initial);
    const controls = createImageControls({
      ...initial, selected: () => value().selected(),
      previous: () => value().previous(), next: () => value().next()
    });
    return { controls, setValue };
  });
  flush();
  const add = vi.spyOn(window, "addEventListener");
  const remove = vi.spyOn(window, "removeEventListener");
  result.setValue(() => current);
  flush();
  for (const key of ["ArrowLeft", "ArrowRight", "0", "+", "-"]) {
    expect(press(key).defaultPrevented).toBe(true);
  }
  expect(initial.previous).not.toHaveBeenCalled();
  expect(initial.next).not.toHaveBeenCalled();
  expect(current.previous).toHaveBeenCalledOnce();
  expect(current.next).toHaveBeenCalledOnce();
  result.setValue(() => ({ ...current, selected: () => ({ ...asset(2), kind: "video" as const }) }));
  flush();
  for (const key of ["0", "+", "-"]) expect(press(key).defaultPrevented).toBe(false);
  const toggleFullscreen = vi.fn(async () => {});
  result.controls.videoPlayerRef({ toggleFullscreen });
  expect(press("f").defaultPrevented).toBe(true);
  expect(toggleFullscreen).toHaveBeenCalledOnce();
  expect(add.mock.calls.filter(([type]) => type === "keydown")).toHaveLength(0);
  expect(remove.mock.calls.filter(([type]) => type === "keydown")).toHaveLength(0);
});

it("consumes fullscreen Escape from an input and suppresses repeated toggles", () => {
  const toggleFullscreen = vi.fn(async () => {});
  const { result } = renderHook(() => createImageControls({ ...options(), selected: () => ({ ...asset(1), kind: "video" }) }));
  flush();
  result.videoPlayerRef({ toggleFullscreen });
  result.onVideoFullscreenChange(true);
  flush();
  const input = document.createElement("input");
  document.body.append(input);
  try {
    expect(press("Escape", input).defaultPrevented).toBe(true);
    expect(press("Escape", input, true).defaultPrevented).toBe(true);
    expect(toggleFullscreen).toHaveBeenCalledOnce();
    result.onVideoFullscreenChange(false);
    flush();
    expect(press("Escape", input).defaultPrevented).toBe(false);
  } finally { input.remove(); }
});

it("stops shortcuts when disabled, closed or disposed and reopens with one listener", () => {
  const next = vi.fn();
  const mount = () => renderHook(() => {
    const [active, setActive] = createSignal(true);
    const [enabled, setEnabled] = createSignal(true);
    createImageControls({ ...options(), active, keyboardEnabled: enabled, next });
    return { setActive, setEnabled };
  });
  const first = mount();
  flush();
  expect(press("ArrowRight").defaultPrevented).toBe(true);
  expect(next).toHaveBeenCalledTimes(1);
  first.result.setEnabled(false);
  flush();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
  first.result.setEnabled(true);
  flush();
  press("ArrowRight");
  expect(next).toHaveBeenCalledTimes(2);
  first.result.setActive(false);
  flush();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
  first.result.setActive(true);
  flush();
  press("ArrowRight");
  expect(next).toHaveBeenCalledTimes(3);
  first.cleanup();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
  const reopened = mount();
  flush();
  press("ArrowRight");
  expect(next).toHaveBeenCalledTimes(4);
  reopened.cleanup();
  expect(press("ArrowRight").defaultPrevented).toBe(false);
});
