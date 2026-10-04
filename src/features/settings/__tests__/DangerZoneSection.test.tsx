import { renderSettings as render } from "./renderSettings";
import { cleanup, screen } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../../components/settings/services/operationStateService";
import { DangerZoneSection } from "../sections/DangerZoneSection";
describe("DangerZoneSection", () => {
  afterEach(() => {
    cleanup();
  });
  it("opens clear confirmation callback", async () => {
    const onOpenClearConfirm = vi.fn();
    render(() => (
      <DangerZoneSection
        isOperationLocked={false}
        operationState={createSectionOperationState("danger")}
        onOpenClearConfirm={onOpenClearConfirm}
      />
    ));
    await userEvent.click(
      screen.getByRole("button", {
        name: "Remove all thumbnails, indexed assets and scan paths"
      })
    );
    expect(onOpenClearConfirm).toHaveBeenCalledTimes(1);
  });
  it("disables dangerous action while locked", () => {
    render(() => (
      <DangerZoneSection
        isOperationLocked
        operationState={createSectionOperationState("danger")}
        onOpenClearConfirm={() => {}}
      />
    ));
    expect(
      screen.getByRole("button", {
        name: "Remove all thumbnails, indexed assets and scan paths"
      })
    ).toBeDisabled();
  });
});
