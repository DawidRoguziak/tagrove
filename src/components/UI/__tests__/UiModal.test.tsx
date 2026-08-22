import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UiModal } from "../UiModal";
import { UiLayerProvider } from "../UiLayerProvider";

describe("UiModal", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not render when closed", () => {
    render(
      <UiModal open={false} onClose={() => {}} ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("closes on overlay click by default", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    fireEvent.mouseDown(screen.getByRole("dialog").parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close on overlay click when disabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} closeOnOverlayClick={false} ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes on Escape when enabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close on Escape when disabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} closeOnEscape={false} ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    const event = new KeyboardEvent("keydown", { key: "Escape", cancelable: true });
    window.dispatchEvent(event);
    expect(onClose).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("does not close when clicking inside content", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} ariaLabel="Test modal">
        <button type="button">inside</button>
      </UiModal>
    );

    const dialog = screen.getByRole("dialog");
    const content = dialog.firstElementChild as HTMLElement;
    fireEvent.mouseDown(content);

    expect(onClose).not.toHaveBeenCalled();
  });

  it("applies size classes", () => {
    const { rerender } = render(
      <UiModal open onClose={() => {}} size="small" ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );

    let content = screen.getByRole("dialog");
    expect(content).toHaveClass("max-w-[clamp(20rem,88vw,28.75rem)]");

    rerender(
      <UiModal open onClose={() => {}} size="medium" ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );
    content = screen.getByRole("dialog");
    expect(content).toHaveClass("max-w-[clamp(24rem,90vw,32.5rem)]");

    rerender(
      <UiModal open onClose={() => {}} size="large" ariaLabel="Test modal">
        <div>content</div>
      </UiModal>
    );
    content = screen.getByRole("dialog");
    expect(content).toHaveClass("max-w-[clamp(28rem,92vw,68rem)]");
  });

  it("lets only the top dialog handle Escape and backdrop dismissal", () => {
    const onLowerClose = vi.fn();
    const onTopClose = vi.fn();
    render(
      <UiLayerProvider>
        <UiModal open onClose={onLowerClose} ariaLabel="Lower" testId="lower-layer">
          <button type="button">Lower action</button>
        </UiModal>
        <UiModal open onClose={onTopClose} ariaLabel="Top" testId="top-layer">
          <button type="button">Top action</button>
        </UiModal>
      </UiLayerProvider>
    );

    fireEvent.mouseDown(screen.getByTestId("lower-layer"));
    expect(onLowerClose).not.toHaveBeenCalled();
    fireEvent.mouseDown(screen.getByTestId("top-layer"));
    expect(onTopClose).toHaveBeenCalledTimes(1);
    onTopClose.mockClear();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onTopClose).toHaveBeenCalledTimes(1);
    expect(onLowerClose).not.toHaveBeenCalled();
  });

  it("does not dismiss a lower layer through a locked top dialog", () => {
    const onLowerClose = vi.fn();
    const onTopClose = vi.fn();
    render(
      <UiLayerProvider>
        <UiModal open onClose={onLowerClose} ariaLabel="Lower">
          lower
        </UiModal>
        <UiModal open onClose={onTopClose} closeOnEscape={false} ariaLabel="Locked top">
          top
        </UiModal>
      </UiLayerProvider>
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onTopClose).not.toHaveBeenCalled();
    expect(onLowerClose).not.toHaveBeenCalled();
  });

  it("traps focus, hides the app root, and restores the trigger", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <UiLayerProvider>
          <button type="button" onClick={() => setOpen(true)}>Open dialog</button>
          <UiModal open={open} onClose={() => setOpen(false)} ariaLabel="Focus dialog">
            <button type="button">First</button>
            <button type="button">Last</button>
          </UiModal>
        </UiLayerProvider>
      );
    }

    const host = document.createElement("div");
    host.id = "root";
    document.body.append(host);
    const rendered = render(<Harness />, { container: host });
    const trigger = screen.getByRole("button", { name: "Open dialog" });
    await userEvent.click(trigger);

    await waitFor(() => expect(screen.getByRole("button", { name: "First" })).toHaveFocus());
    expect(host).toHaveAttribute("inert");
    expect(host).toHaveAttribute("aria-hidden", "true");
    screen.getByRole("button", { name: "First" }).focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();
    screen.getByRole("button", { name: "Last" }).focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(screen.getByRole("button", { name: "First" })).toHaveFocus();

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(host).not.toHaveAttribute("inert");
    expect(host).not.toHaveAttribute("aria-hidden");
    rendered.unmount();
    host.remove();
  });
});
