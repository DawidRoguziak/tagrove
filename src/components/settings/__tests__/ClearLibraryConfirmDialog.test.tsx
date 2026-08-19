import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ClearLibraryConfirmDialog } from "../ClearLibraryConfirmDialog";

describe("ClearLibraryConfirmDialog", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not render when dialog is closed", () => {
    render(
      <ClearLibraryConfirmDialog
        open={false}
        isOperationLocked={false}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("runs confirm and close callbacks", async () => {
    const onClose = vi.fn();
    const onConfirm = vi.fn();

    render(
      <ClearLibraryConfirmDialog
        open
        isOperationLocked={false}
        onClose={onClose}
        onConfirm={onConfirm}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "YES" }));
    await userEvent.click(screen.getByRole("button", { name: "No" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("disables actions when operation is locked", () => {
    render(
      <ClearLibraryConfirmDialog
        open
        isOperationLocked
        onClose={() => {}}
        onConfirm={() => {}}
      />
    );

    expect(screen.getByRole("button", { name: "YES" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "No" })).toBeDisabled();
  });
});
