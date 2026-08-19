import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../services/operationStateService";
import { DangerZoneSection } from "../DangerZoneSection";

describe("DangerZoneSection", () => {
  afterEach(() => {
    cleanup();
  });

  it("opens clear confirmation callback", async () => {
    const onOpenClearConfirm = vi.fn();

    render(
      <DangerZoneSection
        isOperationLocked={false}
        operationState={createSectionOperationState("danger")}
        onOpenClearConfirm={onOpenClearConfirm}
      />
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: "Remove all thumbnails, indexed assets and scan paths"
      })
    );

    expect(onOpenClearConfirm).toHaveBeenCalledTimes(1);
  });

  it("disables dangerous action while locked", () => {
    render(
      <DangerZoneSection
        isOperationLocked
        operationState={createSectionOperationState("danger")}
        onOpenClearConfirm={() => {}}
      />
    );

    expect(
      screen.getByRole("button", {
        name: "Remove all thumbnails, indexed assets and scan paths"
      })
    ).toBeDisabled();
  });
});
