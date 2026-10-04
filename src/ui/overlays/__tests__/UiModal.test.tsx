import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UiModal, type UiModalSize } from "../UiModal";
import { UiLayerProvider } from "../UiLayerProvider";
import { createModalSnapshot } from "../createModalPresence";

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
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
  );
  Object.defineProperty(HTMLElement.prototype, "animate", { configurable: true, value: animate });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  Reflect.deleteProperty(HTMLElement.prototype, "animate");
});
function tick(ms: number) {
  vi.advanceTimersByTime(ms);
  flush();
}
function harness(initial = true) {
  const [open, setOpen] = createSignal(initial);
  const [value, setValue] = createSignal("visible draft");
  const close = vi.fn(() => setOpen(false));
  const [size, setSize] = createSignal<UiModalSize>("small");
  function Content() {
    const visible = createModalSnapshot(open, value);
    return (
      <UiLayerProvider>
        <UiModal open={open()} onClose={close} ariaLabel="Presence" testId="presence" size={size()}>
          <input aria-label="Draft" value={visible()} readonly />
        </UiModal>
      </UiLayerProvider>
    );
  }
  const view = render(() => <Content />);
  return { ...view, setOpen, setValue, close, setSize };
}
describe("Solid modal", () => {
  it("does not render closed and updates size without remounting", () => {
    const view = harness(false);
    expect(screen.queryByRole("dialog")).toBeNull();
    view.setOpen(true);
    flush();
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveClass("max-w-[clamp(20rem,88vw,28.75rem)]");
    view.setSize("medium");
    flush();
    expect(dialog).toHaveClass("max-w-[clamp(24rem,90vw,32.5rem)]");
    view.setSize("large");
    flush();
    expect(dialog).toHaveClass("max-w-[clamp(28rem,92vw,68rem)]");
  });
  it("retains visible values for 100 ms while releasing locks and shielding pointers", () => {
    const view = harness();
    tick(150);
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
    view.setOpen(false);
    view.setValue("");
    flush();
    const overlay = screen.getByTestId("presence");
    expect(overlay).toHaveAttribute("data-modal-presence", "closing");
    expect(overlay).toHaveAttribute("aria-hidden", "true");
    expect(overlay).not.toHaveAttribute("inert");
    expect(overlay.querySelector('[role="dialog"]')).toHaveAttribute("inert");
    expect(overlay.querySelector("input")).toHaveValue("visible draft");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.body.style.overflow).toBe("");
    fireEvent.mouseDown(overlay);
    expect(view.close).not.toHaveBeenCalled();
    expect(animate).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ duration: 100 })
    );
    tick(99);
    expect(overlay).toBeInTheDocument();
    tick(1);
    expect(overlay).not.toBeInTheDocument();
  });
  it("reads seconds from production motion tokens", () => {
    const computed = window.getComputedStyle;
    vi.spyOn(window, "getComputedStyle").mockImplementation((el) => {
      const style = computed(el);
      style.setProperty("--motion-entrance", "0.15s");
      style.setProperty("--motion-exit", "0.1s");
      return style;
    });
    const view = harness();
    expect(animate).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ duration: 150 })
    );
    view.setOpen(false);
    flush();
    expect(animate).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ duration: 100 })
    );
  });
  it("ignores stale animation completion on rapid reopen and cleans timers on disposal", () => {
    const view = harness();
    const entrance = animations[animations.length - 1]?.onfinish;
    tick(25);
    view.setOpen(false);
    flush();
    const exit = animations[animations.length - 1]?.onfinish;
    tick(25);
    view.setOpen(true);
    view.setValue("new draft");
    flush();
    entrance?.();
    exit?.();
    tick(100);
    expect(screen.getByLabelText("Draft")).toHaveValue("new draft");
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "opening");
    tick(50);
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
    view.unmount();
    expect(animations.every((a) => a.cancel.mock.calls.length > 0 && a.onfinish === null)).toBe(
      true
    );
    expect(vi.getTimerCount()).toBe(0);
  });
  it("honors reduced motion and live changes, disposing its subscription", () => {
    let reduced = true;
    let notify = () => {};
    const remove = vi.fn();
    vi.stubGlobal("matchMedia", () => ({
      get matches() {
        return reduced;
      },
      addEventListener: (_: string, fn: () => void) => {
        notify = fn;
      },
      removeEventListener: remove
    }));
    const view = harness();
    expect(screen.getByTestId("presence")).toHaveAttribute("data-modal-presence", "open");
    expect(animate).not.toHaveBeenCalled();
    view.setOpen(false);
    flush();
    expect(screen.queryByTestId("presence")).toBeNull();
    reduced = false;
    notify();
    view.setOpen(true);
    flush();
    view.setOpen(false);
    flush();
    expect(screen.getByTestId("presence")).toBeInTheDocument();
    reduced = true;
    notify();
    flush();
    expect(screen.queryByTestId("presence")).toBeNull();
    view.unmount();
    expect(remove).toHaveBeenCalledWith("change", expect.any(Function));
  });
  it("only dismisses the top dialog and restores nested focus during exit", () => {
    const lower = vi.fn();
    const upper = vi.fn();
    const [open, setOpen] = createSignal(true);
    render(() => (
      <UiLayerProvider>
        <UiModal open ariaLabel="Lower" onClose={lower} testId="lower">
          <button id="trigger" type="button">
            Nested trigger
          </button>
        </UiModal>
        <UiModal
          open={open()}
          ariaLabel="Upper"
          onClose={upper}
          testId="upper"
          getRestoreFocus={() => document.getElementById("trigger")}
        >
          <button type="button">Upper action</button>
        </UiModal>
      </UiLayerProvider>
    ));
    tick(150);
    fireEvent.mouseDown(screen.getByTestId("lower"));
    expect(lower).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByTestId("upper"));
    expect(upper).toHaveBeenCalledOnce();
    upper.mockClear();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(upper).toHaveBeenCalledOnce();
    expect(lower).not.toHaveBeenCalled();
    setOpen(false);
    flush();
    tick(20);
    expect(screen.getByRole("button", { name: "Nested trigger" })).toHaveFocus();
    expect(screen.getByTestId("upper")).not.toHaveAttribute("inert");
    expect(document.body.style.overflow).toBe("hidden");
  });
  it("locks Escape and backdrop without allowing lower layers to dismiss", () => {
    const lower = vi.fn(),
      upper = vi.fn();
    render(() => (
      <UiLayerProvider>
        <UiModal open ariaLabel="Lower" onClose={lower}>
          lower
        </UiModal>
        <UiModal
          open
          ariaLabel="Locked"
          onClose={upper}
          closeOnEscape={false}
          closeOnOverlayClick={false}
          testId="upper"
        >
          upper
        </UiModal>
      </UiLayerProvider>
    ));
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.mouseDown(screen.getByTestId("upper"));
    expect(lower).not.toHaveBeenCalled();
    expect(upper).not.toHaveBeenCalled();
  });
  it("traps focus, preserves preexisting root attributes and restores its trigger", () => {
    const host = document.createElement("div");
    host.id = "root";
    host.setAttribute("aria-hidden", "false");
    document.body.append(host);
    const [open, setOpen] = createSignal(false);
    const view = render(
      () => (
        <UiLayerProvider>
          <button type="button" onClick={() => setOpen(true)}>
            Open
          </button>
          <UiModal open={open()} onClose={() => setOpen(false)} ariaLabel="Focus">
            <button type="button">First</button>
            <button type="button">Last</button>
          </UiModal>
        </UiLayerProvider>
      ),
      { container: host }
    );
    const trigger = screen.getByText("Open");
    trigger.focus();
    fireEvent.click(trigger);
    flush();
    tick(20);
    const first = screen.getByText("First"),
      last = screen.getByText("Last");
    expect(first).toHaveFocus();
    expect(host).toHaveAttribute("inert");
    expect(host).toHaveAttribute("aria-hidden", "true");
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(last).toHaveFocus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(first).toHaveFocus();
    fireEvent.mouseDown(first);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    flush();
    tick(20);
    expect(trigger).toHaveFocus();
    expect(host).not.toHaveAttribute("inert");
    expect(host).toHaveAttribute("aria-hidden", "false");
    view.unmount();
    host.remove();
  });
  it("supports Escape without a provider and removes the listener", () => {
    const close = vi.fn();
    const view = render(() => (
      <UiModal open ariaLabel="Standalone" onClose={close}>
        content
      </UiModal>
    ));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(close).toHaveBeenCalledOnce();
    view.unmount();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(close).toHaveBeenCalledOnce();
  });
});
