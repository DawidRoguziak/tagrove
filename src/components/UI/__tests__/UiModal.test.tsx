import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UiModal } from "../UiModal";

describe("UiModal", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not render when closed", () => {
    render(
      <UiModal open={false} onClose={() => {}}>
        <div>content</div>
      </UiModal>
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("closes on overlay click by default", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose}>
        <div>content</div>
      </UiModal>
    );

    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close on overlay click when disabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} closeOnOverlayClick={false}>
        <div>content</div>
      </UiModal>
    );

    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes on Escape when enabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose}>
        <div>content</div>
      </UiModal>
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close on Escape when disabled", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose} closeOnEscape={false}>
        <div>content</div>
      </UiModal>
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does not close when clicking inside content", () => {
    const onClose = vi.fn();

    render(
      <UiModal open onClose={onClose}>
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
      <UiModal open onClose={() => {}} size="small">
        <div>content</div>
      </UiModal>
    );

    let content = screen.getByRole("dialog").firstElementChild as HTMLElement;
    expect(content).toHaveClass("max-w-[clamp(20rem,88vw,28.75rem)]");

    rerender(
      <UiModal open onClose={() => {}} size="medium">
        <div>content</div>
      </UiModal>
    );
    content = screen.getByRole("dialog").firstElementChild as HTMLElement;
    expect(content).toHaveClass("max-w-[clamp(24rem,90vw,32.5rem)]");

    rerender(
      <UiModal open onClose={() => {}} size="large">
        <div>content</div>
      </UiModal>
    );
    content = screen.getByRole("dialog").firstElementChild as HTMLElement;
    expect(content).toHaveClass("max-w-[clamp(28rem,92vw,68rem)]");
  });
});
