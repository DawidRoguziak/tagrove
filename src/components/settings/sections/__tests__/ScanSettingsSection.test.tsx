import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../services/operationStateService";
import { ScanSettingsSection } from "../ScanSettingsSection";

describe("ScanSettingsSection", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders roots and triggers all scan actions", async () => {
    const onPickFolder = vi.fn();
    const onRescanRoot = vi.fn();
    const onRemoveRoot = vi.fn();
    const onRescanAll = vi.fn();
    const onRenderAllThumbnails = vi.fn();
    const onRenderFailedThumbnails = vi.fn();
    const onCancelThumbnailRender = vi.fn();

    render(
      <ScanSettingsSection
        scanRoots={["C:/media", "C:/video"]}
        isOperationLocked={false}
        thumbnailBulkRunning
        cancelThumbnailRunning={false}
        operationState={createSectionOperationState("scan")}
        onPickFolder={onPickFolder}
        onRescanRoot={onRescanRoot}
        onRemoveRoot={onRemoveRoot}
        onRescanAll={onRescanAll}
        onRenderAllThumbnails={onRenderAllThumbnails}
        onRenderFailedThumbnails={onRenderFailedThumbnails}
        onCancelThumbnailRender={onCancelThumbnailRender}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Choose folder" }));
    await userEvent.click(screen.getByRole("button", { name: "Rescan all" }));
    await userEvent.click(screen.getAllByRole("button", { name: "Rescan" })[0]);
    await userEvent.click(screen.getByRole("button", { name: "Render thumbnails for all indexed assets" }));
    await userEvent.click(screen.getByRole("button", { name: "Retry failed thumbnails only" }));
    await userEvent.click(screen.getByRole("button", { name: "Stop thumbnail render" }));

    expect(onPickFolder).toHaveBeenCalledTimes(1);
    expect(onRescanAll).toHaveBeenCalledTimes(1);
    expect(onRescanRoot).toHaveBeenCalledWith("C:/media");
    expect(onRenderAllThumbnails).toHaveBeenCalledTimes(1);
    expect(onRenderFailedThumbnails).toHaveBeenCalledTimes(1);
    expect(onCancelThumbnailRender).toHaveBeenCalledTimes(1);

    await userEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]);
    expect(onRemoveRoot).toHaveBeenCalledWith("C:/media");
  });

  it("disables controls when locked and without roots", () => {
    render(
      <ScanSettingsSection
        scanRoots={[]}
        isOperationLocked
        thumbnailBulkRunning={false}
        cancelThumbnailRunning={false}
        operationState={createSectionOperationState("scan")}
        onPickFolder={() => {}}
        onRescanRoot={() => {}}
        onRemoveRoot={() => {}}
        onRescanAll={() => {}}
        onRenderAllThumbnails={() => {}}
        onRenderFailedThumbnails={() => {}}
        onCancelThumbnailRender={() => {}}
      />
    );

    expect(screen.getByText("No scan paths")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Rescan all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Render thumbnails for all indexed assets" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry failed thumbnails only" })).toBeDisabled();
  });
});
