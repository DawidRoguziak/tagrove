import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ButtonHTMLAttributes } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TagListSearchLauncher } from "../TagListSearchLauncher";

vi.mock("../../UI/UiIconButton", () => ({
  UiIconButton: ({
    children,
    icon: _icon,
    type = "button",
    ...props
  }: ButtonHTMLAttributes<HTMLButtonElement> & { icon?: string }) => (
    <button type={type} {...props}>
      {children}
    </button>
  )
}));

vi.mock("../TagListModal", () => ({
  TagListModal: ({
    open,
    knownTags,
    onApplySearch,
    onClose
  }: {
    open: boolean;
    knownTags?: string[];
    onApplySearch?: (filterInput: string) => Promise<void> | void;
    onClose: () => void;
  }) =>
    open ? (
      <div data-testid="tag-list-modal">
        <span data-testid="tag-list-known-tags">{knownTags?.join(",") ?? ""}</span>
        <button type="button" onClick={() => void onApplySearch?.("cats dogs")}>
          apply from modal
        </button>
        <button type="button" onClick={onClose}>
          close modal
        </button>
      </div>
    ) : null
}));

describe("TagListSearchLauncher", () => {
  afterEach(() => {
    cleanup();
  });

  it("opens the modal and passes known tags into it", () => {
    render(<TagListSearchLauncher knownTags={["alpha", "beta"]} onApplySearch={vi.fn()} />);

    expect(screen.queryByTestId("tag-list-modal")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Open tag list" }));

    expect(screen.getByTestId("tag-list-modal")).toBeInTheDocument();
    expect(screen.getByTestId("tag-list-known-tags")).toHaveTextContent("alpha,beta");
  });

  it("forwards apply events and lets the modal close itself", () => {
    const onApplySearch = vi.fn();

    render(<TagListSearchLauncher knownTags={[]} onApplySearch={onApplySearch} />);

    fireEvent.click(screen.getByRole("button", { name: "Open tag list" }));
    fireEvent.click(screen.getByRole("button", { name: "apply from modal" }));

    expect(onApplySearch).toHaveBeenCalledWith("cats dogs");

    fireEvent.click(screen.getByRole("button", { name: "close modal" }));

    expect(screen.queryByTestId("tag-list-modal")).not.toBeInTheDocument();
  });
});
