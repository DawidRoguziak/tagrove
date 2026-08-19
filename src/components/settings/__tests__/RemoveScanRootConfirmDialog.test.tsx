import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RemoveScanRootConfirmDialog } from "../RemoveScanRootConfirmDialog";

describe("RemoveScanRootConfirmDialog", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not render when no path is selected", () => {
    render(
      <RemoveScanRootConfirmDialog
        path={null}
        isOperationLocked={false}
        onCancel={() => {}}
        onConfirm={() => {}}
      />
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("runs confirm and cancel callbacks", async () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();

    render(
      <RemoveScanRootConfirmDialog
        path="C:/media"
        isOperationLocked={false}
        onCancel={onCancel}
        onConfirm={onConfirm}
      />
    );

    expect(screen.getByText("C:/media")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("disables actions when operation is locked", () => {
    render(
      <RemoveScanRootConfirmDialog
        path="C:/media"
        isOperationLocked
        onCancel={() => {}}
        onConfirm={() => {}}
      />
    );

    expect(screen.getByRole("button", { name: "Remove" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  });
});
