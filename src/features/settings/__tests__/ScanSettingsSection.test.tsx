import { renderSettings as render } from "./renderSettings";
import { cleanup, screen, within } from "@solidjs/testing-library";
import type { ComponentProps } from "solid-js";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../operationStateService";
import { ScanSettingsSection } from "../sections/ScanSettingsSection";
import { renderSolid } from "../../../test/renderSolid";

const roots = ["/synthetic/zebra", "/synthetic/alpha", "/synthetic/media", "/synthetic/video", "/synthetic/photos"]
  .map((path) => ({ path, auto_scan_on_startup: false }));

function sectionProps(): ComponentProps<typeof ScanSettingsSection> {
  return {
    scanRoots: roots,
    isOperationLocked: false,
    thumbnailBulkRunning: false,
    cancelThumbnailRunning: false,
    operationState: createSectionOperationState("scan"),
    onSetAutoScan: vi.fn(),
    onPickFolder: vi.fn(),
    onRescanRoot: vi.fn(),
    onRemoveRoot: vi.fn(),
    onRescanAll: vi.fn(),
    onRenderAllThumbnails: vi.fn(),
    onRenderFailedThumbnails: vi.fn(),
    onCancelThumbnailRender: vi.fn()
  };
}

function visiblePaths() {
  return screen.queryAllByRole("listitem").map((row) => row.querySelector("[title]")?.textContent);
}
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
    render(() => (
      <ScanSettingsSection
        scanRoots={[
          { path: "C:/media", auto_scan_on_startup: false },
          { path: "C:/video", auto_scan_on_startup: false }
        ]}
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
    ));
    const checkbox = screen.getByRole("checkbox", { name: "Scan on app startup: C:/media" });
    expect(checkbox).not.toBeChecked();
    await userEvent.click(checkbox);
    expect(onSetAutoScan).toHaveBeenCalledWith("C:/media", true);
    expect(onRescanRoot).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Choose folder" }));
    await userEvent.click(screen.getByRole("button", { name: "Rescan all" }));
    const mediaRow = within(screen.getAllByRole("listitem")[0]);
    expect(mediaRow.getByText("C:/media")).toBeInTheDocument();
    await userEvent.click(mediaRow.getByRole("button", { name: "Rescan" }));
    await userEvent.click(
      screen.getByRole("button", { name: "Render thumbnails for all indexed assets" })
    );
    await userEvent.click(screen.getByRole("button", { name: "Retry failed thumbnails only" }));
    await userEvent.click(screen.getByRole("button", { name: "Stop thumbnail render" }));
    expect(onPickFolder).toHaveBeenCalledTimes(1);
    expect(onRescanAll).toHaveBeenCalledTimes(1);
    expect(onRescanRoot).toHaveBeenCalledWith("C:/media");
    expect(onRenderAllThumbnails).toHaveBeenCalledTimes(1);
    expect(onRenderFailedThumbnails).toHaveBeenCalledTimes(1);
    expect(onCancelThumbnailRender).toHaveBeenCalledTimes(1);
    await userEvent.click(mediaRow.getByRole("button", { name: "Remove" }));
    expect(onRemoveRoot).toHaveBeenCalledWith("C:/media");
  });
  it.each([0, 1, 2, 3])("shows all %i roots without a toggle", (count) => {
    renderSolid(ScanSettingsSection, { ...sectionProps(), scanRoots: roots.slice(0, count) });
    expect(visiblePaths()).toEqual(roots.slice(0, count).map((root) => root.path));
    expect(screen.queryByRole("button", { name: /Show more|Show less/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rescan all" }).hasAttribute("disabled")).toBe(count === 0);
    if (count === 0) expect(screen.getByText("No scan paths")).toBeInTheDocument();
  });

  it("expands and collapses in controller order with accessible list controls", async () => {
    const props = sectionProps();
    renderSolid(ScanSettingsSection, props);
    expect(visiblePaths()).toEqual(roots.slice(0, 3).map((root) => root.path));
    const toggle = screen.getByRole("button", { name: "Show more" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAttribute("aria-controls", screen.getByRole("list").id);
    expect(screen.getByRole("list").id).not.toBe("");
    await userEvent.click(screen.getByRole("button", { name: "Rescan all" }));
    expect(props.onRescanAll).toHaveBeenCalledTimes(1);
    await userEvent.click(toggle);
    expect(screen.getByRole("button", { name: "Show less" })).toBe(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAttribute("aria-controls", screen.getByRole("list").id);
    expect(visiblePaths()).toEqual(roots.map((root) => root.path));

    const revealedRow = within(screen.getAllByRole("listitem")[4]);
    await userEvent.click(revealedRow.getByRole("checkbox"));
    await userEvent.click(revealedRow.getByRole("button", { name: "Rescan" }));
    await userEvent.click(revealedRow.getByRole("button", { name: "Remove" }));
    expect(props.onSetAutoScan).toHaveBeenCalledExactlyOnceWith(roots[4].path, true);
    expect(props.onRescanRoot).toHaveBeenCalledExactlyOnceWith(roots[4].path);
    expect(props.onRemoveRoot).toHaveBeenCalledExactlyOnceWith(roots[4].path);

    await userEvent.click(toggle);
    expect(screen.getByRole("button", { name: "Show more" })).toHaveAttribute("aria-expanded", "false");
    expect(visiblePaths()).toEqual(roots.slice(0, 3).map((root) => root.path));
  });

  it("preserves expansion through live root refreshes and threshold changes", async () => {
    const props = sectionProps();
    const view = renderSolid(ScanSettingsSection, props);
    const refreshed = [...roots].reverse();
    view.rerender({ ...props, scanRoots: refreshed });
    expect(visiblePaths()).toEqual(refreshed.slice(0, 3).map((root) => root.path));
    await userEvent.click(screen.getByRole("button", { name: "Show more" }));
    view.rerender({ ...props, scanRoots: refreshed.slice(1) });
    expect(visiblePaths()).toEqual(refreshed.slice(1).map((root) => root.path));
    expect(screen.getByRole("button", { name: "Show less" })).toHaveAttribute("aria-expanded", "true");
    for (const scanRoots of [roots.slice(0, 2), []]) {
      view.rerender({ ...props, scanRoots });
      expect(visiblePaths()).toEqual(scanRoots.map((root) => root.path));
      expect(screen.queryByRole("button", { name: /Show more|Show less/ })).not.toBeInTheDocument();
    }
    view.rerender(props);
    expect(visiblePaths()).toEqual(roots.map((root) => root.path));
    await userEvent.click(screen.getByRole("button", { name: "Show less" }));
    view.rerender({ ...props, scanRoots: refreshed });
    expect(visiblePaths()).toEqual(refreshed.slice(0, 3).map((root) => root.path));
  });

  it("keeps the display toggle usable while operations and row actions are locked", async () => {
    const props = sectionProps();
    const view = renderSolid(ScanSettingsSection, props);
    view.rerender({ ...props, isOperationLocked: true });
    const toggle = screen.getByRole("button", { name: "Show more" });
    expect(toggle).toBeEnabled();
    await userEvent.click(toggle);
    expect(visiblePaths()).toHaveLength(5);
    for (const row of screen.getAllByRole("listitem")) {
      expect(within(row).getByRole("checkbox")).toBeDisabled();
      for (const button of within(row).getAllByRole("button")) expect(button).toBeDisabled();
    }
    expect(screen.getByRole("button", { name: "Rescan all" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Show less" }));
    expect(visiblePaths()).toHaveLength(3);
    expect(props.onRescanRoot).not.toHaveBeenCalled();
    expect(props.onRemoveRoot).not.toHaveBeenCalled();
    expect(props.onSetAutoScan).not.toHaveBeenCalled();
  });

  it("resets expansion after remount", async () => {
    const props = sectionProps();
    const view = renderSolid(ScanSettingsSection, props);
    await userEvent.click(screen.getByRole("button", { name: "Show more" }));
    expect(visiblePaths()).toHaveLength(5);
    view.unmount();
    renderSolid(ScanSettingsSection, props);
    expect(visiblePaths()).toEqual(roots.slice(0, 3).map((root) => root.path));
    expect(screen.getByRole("button", { name: "Show more" })).toHaveAttribute("aria-expanded", "false");
  });
  it("disables controls when locked and without roots", () => {
    render(() => (
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
    ));
    expect(screen.getByText("No scan paths")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Rescan all" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Render thumbnails for all indexed assets" })
    ).toBeDisabled();
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
    render(() => (
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
    ));
    expect(scrollIntoView).toHaveBeenCalledWith({ behavior: "auto", block: "center" });
    HTMLElement.prototype.scrollIntoView = originalScrollIntoView;
  });
});
