import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../services/operationStateService";
import { DuplicatesSection } from "../DuplicatesSection";

describe("DuplicatesSection", () => {
  afterEach(() => {
    cleanup();
  });

  it("starts duplicate scan and shows last summary", async () => {
    const onStart = vi.fn();

    render(
      <DuplicatesSection
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        duplicateGroups={3}
        duplicateAssets={8}
        onStart={onStart}
      />
    );

    expect(screen.getByText("Last result: 3 groups / 8 assets")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(onStart).toHaveBeenCalledTimes(1);
  });

  it("hides summary when no duplicates and disables start while locked", () => {
    render(
      <DuplicatesSection
        isOperationLocked
        operationState={createSectionOperationState("duplicates")}
        duplicateGroups={0}
        duplicateAssets={0}
        onStart={() => {}}
      />
    );

    expect(screen.queryByText(/Last result:/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();
  });
});
