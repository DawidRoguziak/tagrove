import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LightboxDeleteConfirmDialog } from "../LightboxDeleteConfirmDialog";

describe("LightboxDeleteConfirmDialog", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not render when closed", () => {
    render(
      <LightboxDeleteConfirmDialog open={false} isSubmitting={false} onClose={() => {}} onConfirm={() => {}} />
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("enables confirm only when input equals Yes (case-insensitive)", async () => {
    render(<LightboxDeleteConfirmDialog open isSubmitting={false} onClose={() => {}} onConfirm={() => {}} />);

    const confirmButton = screen.getByRole("button", { name: "Confirm" });
    const input = screen.getByLabelText('Type "Yes"');
    expect(confirmButton).toBeDisabled();

    await userEvent.type(input, "no");
    expect(confirmButton).toBeDisabled();

    await userEvent.clear(input);
    await userEvent.type(input, "yEs");
    expect(confirmButton).toBeEnabled();
  });

  it("runs close and confirm callbacks", async () => {
    const onClose = vi.fn();
    const onConfirm = vi.fn();

    render(<LightboxDeleteConfirmDialog open isSubmitting={false} onClose={onClose} onConfirm={onConfirm} />);

    const input = screen.getByLabelText('Type "Yes"');
    await userEvent.type(input, "Yes");
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
