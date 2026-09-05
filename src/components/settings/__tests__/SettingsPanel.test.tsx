import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createSectionOperationState } from "../services/operationStateService";
import { SettingsPanel } from "../SettingsPanel";

function renderPanel(overrides: Partial<ComponentProps<typeof SettingsPanel>> = {}) {
  const props: ComponentProps<typeof SettingsPanel> = {
    theme: "dark",
    onThemeChange: vi.fn(),
    language: "en",
    onLanguageChange: vi.fn(),
    scanRoots: ["C:/media"],
    isOperationLocked: false,
    thumbnailBulkRunning: false,
    cancelThumbnailRunning: false,
    scanOperationState: createSectionOperationState("scan"),
    importExportOperationState: createSectionOperationState("importExport"),
    dangerOperationState: createSectionOperationState("danger"),
    duplicateOperationState: createSectionOperationState("duplicates"),
    duplicateGroups: [],
    duplicateAssetCount: 0,
    removeRootConfirmPath: null,
    clearConfirmOpen: false,
    duplicateDialogOpen: false,
    pendingDuplicateDeleteConfirm: null,
    pendingImportDbSourcePath: null,
    onPickFolder: vi.fn(),
    onRescanRoot: vi.fn(),
    onRemoveRoot: vi.fn(),
    onRescanAll: vi.fn(),
    onRenderAllThumbnails: vi.fn(),
    onRenderFailedThumbnails: vi.fn(),
    onCancelThumbnailRender: vi.fn(),
    onExportCsv: vi.fn(),
    onImportCsv: vi.fn(),
    onExportDbBundle: vi.fn(),
    onImportDbBundle: vi.fn(),
    onCancelRemoveRootConfirm: vi.fn(),
    onConfirmRemoveRoot: vi.fn(),
    onOpenClearConfirm: vi.fn(),
    onCloseClearConfirm: vi.fn(),
    onConfirmClearYes: vi.fn(),
    onStartDuplicateScan: vi.fn(),
    onCloseDuplicateDialog: vi.fn(),
    onRescanDuplicates: vi.fn(),
    onSaveDuplicateChanges: vi.fn(async () => {}),
    onCancelDuplicateDeleteConfirm: vi.fn(),
    onConfirmDuplicateDeleteConfirm: vi.fn(async () => {}),
    onCancelImportDbOverwriteConfirm: vi.fn(),
    onConfirmImportDbOverwriteConfirm: vi.fn(async () => {}),
    ...overrides
  };

  const view = render(<SettingsPanel {...props} />);
  return { ...view, props };
}

describe("SettingsPanel", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders all settings sections", () => {
    renderPanel();

    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Scan settings" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Import / Export" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Danger zone" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Duplicate manager" })).toBeInTheDocument();
  });

  it("wires section action callbacks", async () => {
    const { props } = renderPanel();

    await userEvent.click(screen.getByRole("button", { name: "Choose folder" }));
    await userEvent.click(screen.getByRole("button", { name: "Rescan all" }));
    await userEvent.click(screen.getByRole("button", { name: "Rescan" }));
    await userEvent.click(screen.getByRole("button", { name: "Render thumbnails for all indexed assets" }));
    await userEvent.click(screen.getByRole("button", { name: "Retry failed thumbnails only" }));
    await userEvent.click(screen.getByRole("button", { name: "Export tags CSV" }));
    await userEvent.click(screen.getByRole("button", { name: "Import tags CSV" }));
    await userEvent.click(screen.getByRole("button", { name: "Export DB + thumbnails" }));
    await userEvent.click(screen.getByRole("button", { name: "Import DB + thumbnails" }));
    await userEvent.click(
      screen.getByRole("button", {
        name: "Remove all thumbnails, indexed assets and scan paths"
      })
    );
    await userEvent.click(screen.getByRole("button", { name: "Start" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove" }));

    expect(props.onPickFolder).toHaveBeenCalledTimes(1);
    expect(props.onRescanAll).toHaveBeenCalledTimes(1);
    expect(props.onRescanRoot).toHaveBeenCalledWith("C:/media");
    expect(props.onRenderAllThumbnails).toHaveBeenCalledTimes(1);
    expect(props.onRenderFailedThumbnails).toHaveBeenCalledTimes(1);
    expect(props.onExportCsv).toHaveBeenCalledTimes(1);
    expect(props.onImportCsv).toHaveBeenCalledTimes(1);
    expect(props.onExportDbBundle).toHaveBeenCalledTimes(1);
    expect(props.onImportDbBundle).toHaveBeenCalledTimes(1);
    expect(props.onOpenClearConfirm).toHaveBeenCalledTimes(1);
    expect(props.onStartDuplicateScan).toHaveBeenCalledTimes(1);
    expect(props.onRemoveRoot).toHaveBeenCalledWith("C:/media");
  });

  it("renders clear confirmation dialog when enabled", async () => {
    const onCloseClearConfirm = vi.fn();
    const onConfirmClearYes = vi.fn();

    renderPanel({
      clearConfirmOpen: true,
      onCloseClearConfirm,
      onConfirmClearYes
    });

    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "YES" }));
    await userEvent.click(screen.getByRole("button", { name: "No" }));

    expect(onConfirmClearYes).toHaveBeenCalledTimes(1);
    expect(onCloseClearConfirm).toHaveBeenCalledTimes(1);
  });

  it("renders remove root confirmation dialog when enabled", async () => {
    const onCancelRemoveRootConfirm = vi.fn();
    const onConfirmRemoveRoot = vi.fn();

    renderPanel({
      removeRootConfirmPath: "C:/media",
      onCancelRemoveRootConfirm,
      onConfirmRemoveRoot
    });

    expect(screen.getByRole("heading", { name: "Remove scan path?" })).toBeInTheDocument();
    const dialog = screen.getByRole("dialog");

    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onConfirmRemoveRoot).toHaveBeenCalledTimes(1);
    expect(onCancelRemoveRootConfirm).toHaveBeenCalledTimes(1);
  });

  it("navigates to mounted sections and focuses the destination without changing the URL", async () => {
    const { container } = renderPanel({ fullView: true });
    const appearance = container.querySelector<HTMLElement>("#settings-appearance");
    if (!appearance) throw new Error("Missing appearance section");
    const scroll = vi.fn();
    appearance.scrollIntoView = scroll;
    const previousHash = window.location.hash;
    const navigation = screen.getByRole("navigation", { name: "Settings sections" });
    await userEvent.click(within(navigation).getByRole("link", { name: "Appearance" }));
    expect(scroll).toHaveBeenCalledWith({ block: "start", behavior: "instant" });
    expect(appearance).toHaveFocus();
    expect(window.location.hash).toBe(previousHash);
    expect(within(navigation).getByRole("link", { name: "Appearance" })).toHaveAttribute("aria-current", "location");
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeInTheDocument();
  });

  it("renders the canonical confirmation label in destructive dialogs", () => {
    renderPanel({
      pendingDuplicateDeleteConfirm: {
        changes: [{ assetId: 1, type: "delete" }],
        deleteCount: 1
      }
    });

    let dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Confirm" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeInTheDocument();

    cleanup();
    renderPanel({ pendingImportDbSourcePath: "C:/backup.zip" });

    dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Confirm" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeInTheDocument();
  });
});
