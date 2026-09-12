import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UiModal } from "../UiModal";
import { UiLayerProvider } from "../UiLayerProvider";
import { RemoveScanRootConfirmDialog } from "../../settings/RemoveScanRootConfirmDialog";

describe("modal presence", () => {
  const animations: { onfinish: (() => void) | null; cancel: ReturnType<typeof vi.fn> }[] = [];
  const animate = vi.fn(() => {
    const animation = { onfinish: null as (() => void) | null, cancel: vi.fn() };
    animations.push(animation);
    return animation;
  });
  beforeEach(() => {
    vi.useFakeTimers();
    animations.length = 0;
    animate.mockClear();
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })));
    Object.defineProperty(HTMLElement.prototype, "animate", { configurable: true, value: animate });
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "animate");
  });

  function modal(open: boolean, value = "visible draft") {
    return <StrictMode><UiLayerProvider>
      <UiModal open={open} onClose={vi.fn()} ariaLabel="Presence" testId="presence">
        <input aria-label="Draft" value={value} readOnly />
      </UiModal>
    </UiLayerProvider></StrictMode>;
  }

  it("retains the visible draft for 100 ms but releases modality on close", () => {
    const { rerender } = render(modal(true));
    act(() => vi.advanceTimersByTime(150));
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
    rerender(modal(false, ""));
    const overlay = screen.getByTestId("presence");
    expect(overlay).toHaveAttribute("data-modal-presence", "closing");
    expect(overlay).toHaveAttribute("aria-hidden", "true");
    expect(overlay).not.toHaveAttribute("inert");
    expect(overlay.querySelector('[role="dialog"]')).toHaveAttribute("inert");
    expect(overlay.querySelector("input")).toHaveValue("visible draft");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.body.style.overflow).toBe("");
    expect(animate).toHaveBeenLastCalledWith(expect.anything(), expect.objectContaining({ duration: 100 }));
    act(() => vi.advanceTimersByTime(99));
    expect(overlay).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(overlay).not.toBeInTheDocument();
  });

  it("reads production CSS duration tokens normalized to seconds", () => {
    document.documentElement.style.setProperty("--motion-entrance", "0.15s");
    document.documentElement.style.setProperty("--motion-exit", "0.1s");
    // jsdom does not inherit custom properties, so apply the compiled tokens to the target.
    const computedStyle = window.getComputedStyle;
    vi.spyOn(window, "getComputedStyle").mockImplementation(element => {
      const style = computedStyle(element);
      style.setProperty("--motion-entrance", "0.15s");
      style.setProperty("--motion-exit", "0.1s");
      return style;
    });
    const { rerender } = render(modal(true));
    expect(animate).toHaveBeenLastCalledWith(expect.anything(), expect.objectContaining({ duration: 150 }));
    rerender(modal(false));
    expect(animate).toHaveBeenLastCalledWith(expect.anything(), expect.objectContaining({ duration: 100 }));
    vi.restoreAllMocks();
    document.documentElement.removeAttribute("style");
  });

  it("cancels entrance and exit callbacks when rapidly reopened", () => {
    const { rerender } = render(modal(true));
    const staleEntrance = animations.at(-1)?.onfinish;
    act(() => vi.advanceTimersByTime(25));
    rerender(modal(false));
    const staleExit = animations.at(-1)?.onfinish;
    act(() => vi.advanceTimersByTime(25));
    rerender(modal(true, "new draft"));
    act(() => { staleEntrance?.(); staleExit?.(); vi.advanceTimersByTime(100); });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByLabelText("Draft")).toHaveValue("new draft");
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "opening");
    act(() => vi.advanceTimersByTime(50));
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
  });

  it("cleans animations and fallback timers on unmount", () => {
    const { unmount } = render(<UiModal open ariaLabel="Unmount" onClose={vi.fn()}>content</UiModal>);
    unmount();
    expect(animations.every(animation => animation.cancel.mock.calls.length > 0 && animation.onfinish === null)).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("shows and removes immediately with reduced motion, including a live preference change", () => {
    let reduced = true;
    let notify = () => {};
    vi.stubGlobal("matchMedia", () => ({
      matches: reduced,
      addEventListener: (_event: string, callback: () => void) => { notify = callback; },
      removeEventListener: vi.fn()
    }));
    const { rerender } = render(modal(true));
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
    expect(animate).not.toHaveBeenCalled();
    rerender(modal(false));
    expect(screen.queryByTestId("presence")).toBeNull();
    act(() => { reduced = false; notify(); });
    rerender(modal(true));
    rerender(modal(false));
    expect(screen.getByTestId("presence")).toBeInTheDocument();
    act(() => { reduced = true; notify(); });
    expect(screen.queryByTestId("presence")).toBeNull();
  });

  it("restores a nested dialog's focus during exit and leaves its backdrop as a pointer shield", () => {
    const onClose = vi.fn();
    const view = (open: boolean) => <UiLayerProvider>
      <UiModal open ariaLabel="Lower" onClose={onClose}><button id="nested-trigger" type="button">Nested trigger</button></UiModal>
      <UiModal open={open} ariaLabel="Upper" onClose={onClose} testId="upper"
        getRestoreFocus={() => document.getElementById("nested-trigger")}><button type="button">Upper action</button></UiModal>
    </UiLayerProvider>;
    const { rerender } = render(view(true));
    act(() => vi.advanceTimersByTime(150));
    rerender(view(false));
    act(() => vi.advanceTimersByTime(20));
    expect(screen.getByRole("button", { name: "Nested trigger" })).toHaveFocus();
    const closing = screen.getByTestId("upper");
    expect(closing).toHaveAttribute("aria-hidden", "true");
    expect(closing).not.toHaveAttribute("inert");
    fireEvent.mouseDown(closing);
    expect(onClose).not.toHaveBeenCalled();
    expect(document.body.style.overflow).toBe("hidden");
  });

  it("preserves a cleared scan path and keeps locked confirmations open", () => {
    const props = { isOperationLocked: true, onCancel: vi.fn(), onConfirm: vi.fn() };
    const { rerender } = render(<RemoveScanRootConfirmDialog {...props} path="/synthetic/media" />);
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.mouseDown(screen.getByRole("dialog").parentElement!);
    expect(props.onCancel).not.toHaveBeenCalled();
    rerender(<RemoveScanRootConfirmDialog {...props} path={null} />);
    expect(screen.getByText("/synthetic/media")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(100));
    expect(screen.queryByText("/synthetic/media")).toBeNull();
  });
});
