import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../services/operationStateService";
import { ImportExportSection } from "../ImportExportSection";

describe("ImportExportSection", () => {
  afterEach(() => {
    cleanup();
  });

  it("triggers import-export actions", async () => {
    const onExportCsv = vi.fn();
    const onImportCsv = vi.fn();
    const onExportDbBundle = vi.fn();
    const onImportDbBundle = vi.fn();

    render(
      <ImportExportSection
        isOperationLocked={false}
        operationState={createSectionOperationState("importExport")}
        onExportCsv={onExportCsv}
        onImportCsv={onImportCsv}
        onExportDbBundle={onExportDbBundle}
        onImportDbBundle={onImportDbBundle}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Export tags CSV" }));
    await userEvent.click(screen.getByRole("button", { name: "Import tags CSV" }));
    await userEvent.click(screen.getByRole("button", { name: "Export DB + thumbnails" }));
    await userEvent.click(screen.getByRole("button", { name: "Import DB + thumbnails" }));

    expect(onExportCsv).toHaveBeenCalledTimes(1);
    expect(onImportCsv).toHaveBeenCalledTimes(1);
    expect(onExportDbBundle).toHaveBeenCalledTimes(1);
    expect(onImportDbBundle).toHaveBeenCalledTimes(1);
  });

  it("disables all actions when operation is locked", () => {
    render(
      <ImportExportSection
        isOperationLocked
        operationState={createSectionOperationState("importExport")}
        onExportCsv={() => {}}
        onImportCsv={() => {}}
        onExportDbBundle={() => {}}
        onImportDbBundle={() => {}}
      />
    );

    expect(screen.getByRole("button", { name: "Export tags CSV" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Import tags CSV" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export DB + thumbnails" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Import DB + thumbnails" })).toBeDisabled();
  });
});
