import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../../services/operationStateService";
import { ScanSettingsSection } from "../ScanSettingsSection";

const originalMatchMedia = window.matchMedia;

describe("ScanSettingsSection", () => {
  afterEach(() => {
    cleanup();
    Object.defineProperty(window, "matchMedia", { configurable: true, value: originalMatchMedia });
  });

  it("renders roots and triggers all scan actions", async () => {
    const onSetAutoScan = vi.fn();
    const onPickFolder = vi.fn();
    const onRescanRoot = vi.fn();
    const onRemoveRoot = vi.fn();
    const onRescanAll = vi.fn();
    const onRenderAllThumbnails = vi.fn();
    const onRenderFailedThumbnails = vi.fn();
    const onCancelThumbnailRender = vi.fn();

    render(
      <ScanSettingsSection
        scanRoots={[{ path: "C:/media", auto_scan_on_startup: false }, { path: "C:/video", auto_scan_on_startup: false }]}
        isOperationLocked={false}
        thumbnailBulkRunning
        cancelThumbnailRunning={false}
        operationState={createSectionOperationState("scan")}
        onSetAutoScan={onSetAutoScan}
        onPickFolder={onPickFolder}
        onRescanRoot={onRescanRoot}
        onRemoveRoot={onRemoveRoot}
        onRescanAll={onRescanAll}
        onRenderAllThumbnails={onRenderAllThumbnails}
        onRenderFailedThumbnails={onRenderFailedThumbnails}
        onCancelThumbnailRender={onCancelThumbnailRender}
      />
    );

    const checkbox = screen.getByRole("checkbox", { name: "Scan on app startup: C:/media" });
    expect(checkbox).not.toBeChecked();
    await userEvent.click(checkbox);
    expect(onSetAutoScan).toHaveBeenCalledWith("C:/media", true);
    expect(onRescanRoot).not.toHaveBeenCalled();
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
        onSetAutoScan={vi.fn()}
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

  it("avoids smooth scrolling when reduced motion is requested", () => {
    const scrollIntoView = vi.fn();
    const originalScrollIntoView = HTMLElement.prototype.scrollIntoView;
    HTMLElement.prototype.scrollIntoView = scrollIntoView;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({ matches: true }))
    });

    render(
      <ScanSettingsSection
        scanRoots={[]}
        highlighted
        isOperationLocked={false}
        thumbnailBulkRunning={false}
        cancelThumbnailRunning={false}
        operationState={createSectionOperationState("scan")}
        onSetAutoScan={vi.fn()}
        onPickFolder={() => {}}
        onRescanRoot={() => {}}
        onRemoveRoot={() => {}}
        onRescanAll={() => {}}
        onRenderAllThumbnails={() => {}}
        onRenderFailedThumbnails={() => {}}
        onCancelThumbnailRender={() => {}}
      />
    );

    expect(scrollIntoView).toHaveBeenCalledWith({ behavior: "auto", block: "center" });
    HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
  });
});
