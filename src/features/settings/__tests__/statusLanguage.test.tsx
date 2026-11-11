import { fireEvent, render, renderHook, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { changeAppLanguage } from "../../../i18n/core";
import { LanguageProvider } from "../../../i18n/solid";
import type { ScanProgress, ScanSummary } from "../../../platform/tauri/contracts/scanning";
import { settle } from "../../../test/settle";
import { createDuplicateSettings } from "../createDuplicateSettings";
import { createScanSettings } from "../createScanSettings";
import { createSettingsOperationRunner } from "../createSettingsOperationRunner";
import { SectionOperationStatus } from "../SectionOperationStatus";
import { ScanSettingsSection } from "../sections/ScanSettingsSection";

const api = vi.hoisted(() => ({
  listen: vi.fn(),
  getVideoToolStatus: vi.fn(),
  listScanRoots: vi.fn(),
  rescanAllRoots: vi.fn(),
  setScanRootAutoScan: vi.fn(),
  findDuplicateAssets: vi.fn(),
  applyDuplicateResolutionBatch: vi.fn()
}));
vi.mock("../../../platform/tauri/events", () => api);
vi.mock("../../../platform/tauri/maintenance", () => api);
vi.mock("../../../platform/tauri/scanning", () => api);
vi.mock("../../../platform/tauri/thumbnails", () => ({}));
vi.mock("../../../platform/tauri/dialogs", () => ({ open: vi.fn() }));

const root = { path: "/synthetic/media", auto_scan_on_startup: false };
let progress: (event: { payload: ScanProgress }) => void;

beforeEach(() => {
  vi.resetAllMocks();
  api.listen.mockImplementation(async (_, handler) => {
    progress = handler;
    return vi.fn();
  });
  api.getVideoToolStatus.mockResolvedValue({ ffmpeg_available: true, ffprobe_available: true });
  // The backend returns fresh objects on every call.
  api.listScanRoots.mockImplementation(async () => [{ ...root }]);
});
afterEach(async () => {
  await changeAppLanguage("en");
});

function setup() {
  const view = renderHook(
    () => {
      const runner = createSettingsOperationRunner({ setLoading: vi.fn() });
      const scan = createScanSettings({
        runner,
        refreshLibrary: async () => {},
        onRootRemoved: vi.fn()
      });
      return { runner, scan };
    },
    { wrapper: LanguageProvider }
  );
  const { runner, scan } = view.result;
  const ui = render(() => (
    <LanguageProvider>
      <ScanSettingsSection
        scanRoots={scan.scanRoots}
        onSetAutoScan={scan.onSetAutoScan}
        isOperationLocked={scan.isOperationLocked}
        thumbnailBulkRunning={scan.thumbnailBulkRunning}
        cancelThumbnailRunning={scan.cancelThumbnailRunning}
        operationState={scan.operationState}
        onPickFolder={scan.onPickFolder}
        onRescanRoot={scan.onRescanRoot}
        onRemoveRoot={scan.onRequestRemoveRoot}
        onRescanAll={scan.onRescanAll}
        onRenderAllThumbnails={scan.onRenderAllThumbnails}
        onRenderFailedThumbnails={scan.onRenderFailedThumbnails}
        onCancelThumbnailRender={scan.onCancelThumbnailRender}
      />
      <SectionOperationStatus state={runner.importExportOperationState} loaderTestId="ie" />
      <SectionOperationStatus state={runner.dangerOperationState} loaderTestId="danger" />
      <SectionOperationStatus state={runner.duplicateOperationState} loaderTestId="dup" />
    </LanguageProvider>
  ));
  return { ui, scan, runner };
}

describe("section status language", () => {
  it("re-translates the idle ready lines after a language switch", async () => {
    setup();
    expect(screen.getByText("Ready for import/export")).toBeInTheDocument();
    expect(screen.getByText("Ready to scan")).toBeInTheDocument();
    await settle(() => changeAppLanguage("pl"));
    expect(screen.getByText("Gotowe do importu/eksportu")).toBeInTheDocument();
    expect(screen.getByText("Gotowe do skanowania")).toBeInTheDocument();
    expect(screen.getByText("Brak uruchomionej operacji destrukcyjnej")).toBeInTheDocument();
    expect(screen.getByText("Gotowe do skanowania duplikatow")).toBeInTheDocument();
    expect(screen.queryByText("Ready for import/export")).not.toBeInTheDocument();
    await settle(() => changeAppLanguage("en"));
    expect(screen.getByText("Ready for import/export")).toBeInTheDocument();
  });

  it("re-translates a finished operation summary after a language switch", async () => {
    api.rescanAllRoots.mockResolvedValue({
      completion: "complete",
      indexed: 3,
      removed: 0,
      failed: 0
    });
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    await settle(() => scan.onRescanAll());
    expect(screen.getByText("Done. Indexed: 3, removed: 0, failed: 0")).toBeInTheDocument();
    await settle(() => changeAppLanguage("pl"));
    expect(screen.getByText("Gotowe. Zindeksowano: 3, usunieto: 0, bledy: 0")).toBeInTheDocument();
    expect(screen.queryByText(/Indexed/)).not.toBeInTheDocument();
  });

  it("re-translates a failed operation prefix and a backend i18n detail", async () => {
    api.rescanAllRoots.mockRejectedValue("i18n:settings.status.ready.danger");
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    await settle(() => scan.onRescanAll());
    expect(screen.getByText("Rescan error: No destructive operation running")).toBeInTheDocument();
    await settle(() => changeAppLanguage("pl"));
    expect(
      screen.getByText("Blad ponownego skanu: Brak uruchomionej operacji destrukcyjnej")
    ).toBeInTheDocument();
  });

  it("re-translates the pending line and the live progress line while an operation runs", async () => {
    let finish!: (summary: ScanSummary) => void;
    api.rescanAllRoots.mockReturnValue(
      new Promise<ScanSummary>((resolve) => {
        finish = resolve;
      })
    );
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    const running = scan.onRescanAll();
    await waitFor(() => expect(api.rescanAllRoots).toHaveBeenCalled());
    expect(screen.getByRole("status")).toHaveTextContent("Rescanning all folders...");
    await settle(() => changeAppLanguage("pl"));
    expect(screen.getByRole("status")).toHaveTextContent(
      "Ponowne skanowanie wszystkich folderow..."
    );
    settle(() =>
      progress({ payload: { phase: "scanning", processed: 2, total: 7, message: "x" } })
    );
    expect(screen.getByRole("status")).toHaveTextContent("Skanowanie 2/7");
    await settle(() => changeAppLanguage("en"));
    expect(screen.getByRole("status")).toHaveTextContent("Scanning 2/7");
    finish({ completion: "complete", indexed: 1, removed: 0, failed: 0 });
    await settle(() => running);
  });
});

describe("duplicate apply summary language", () => {
  it("re-translates the applied-changes summary", async () => {
    api.findDuplicateAssets.mockResolvedValue({
      groups: [{ assets: [{ id: 1, path: "/synthetic/a.jpg", record_version: 3 }] }],
      duplicate_groups: 1,
      duplicate_assets: 2,
      revision: 7
    });
    api.applyDuplicateResolutionBatch.mockResolvedValue({
      status: "committed",
      revision: 8,
      results: [],
      removed_thumbnails: 0
    });
    const { runner, duplicates } = renderHook(
      () => {
        const runner = createSettingsOperationRunner({ setLoading: vi.fn() });
        return {
          runner,
          duplicates: createDuplicateSettings({ runner, refreshLibrary: async () => {} })
        };
      },
      { wrapper: LanguageProvider }
    ).result;
    const ui = render(() => (
      <LanguageProvider>
        <SectionOperationStatus state={runner.duplicateOperationState} loaderTestId="dup" />
      </LanguageProvider>
    ));
    await settle(() => duplicates.onStart());
    await settle(() =>
      duplicates.onSaveAll([{ assetId: 1, type: "rename", nextFileName: "b.jpg" }])
    );
    expect(ui.getByText(/^Applied 1 change\./)).toBeInTheDocument();
    await settle(() => changeAppLanguage("pl"));
    expect(ui.getByText(/^Zastosowano 1 zmianę\./)).toBeInTheDocument();
  });
});

describe("startup scan toggle after a failed save", () => {
  it("shows the saved value again when the canonical roots reload", async () => {
    api.setScanRootAutoScan.mockRejectedValue(new Error("write failed"));
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    const toggle = await screen.findByRole("checkbox", { name: /Scan on app startup/ });
    expect(toggle).not.toBeChecked();
    fireEvent.click(toggle);
    await waitFor(() => expect(api.setScanRootAutoScan).toHaveBeenCalledWith(root.path, true));
    await waitFor(() => expect(screen.getByText(/write failed/)).toBeInTheDocument());
    expect(api.listScanRoots).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("checkbox", { name: /Scan on app startup/ })).not.toBeChecked();
    expect(scan.scanRoots).toEqual([root]);
  });

  it("shows the saved value again when the reload fails as well", async () => {
    api.setScanRootAutoScan.mockRejectedValue(new Error("write failed"));
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    api.listScanRoots.mockRejectedValue(new Error("read failed"));
    fireEvent.click(await screen.findByRole("checkbox", { name: /Scan on app startup/ }));
    await waitFor(() => expect(screen.getByText(/write failed/)).toBeInTheDocument());
    expect(screen.getByRole("checkbox", { name: /Scan on app startup/ })).not.toBeChecked();
    expect(scan.scanRoots).toEqual([root]);
  });

  it("keeps the toggle on after a successful save", async () => {
    api.setScanRootAutoScan.mockResolvedValue(undefined);
    const { scan } = setup();
    await settle(() => scan.refreshScanRoots());
    fireEvent.click(await screen.findByRole("checkbox", { name: /Scan on app startup/ }));
    await waitFor(() => expect(scan.scanRoots[0]?.auto_scan_on_startup).toBe(true));
    expect(screen.getByRole("checkbox", { name: /Scan on app startup/ })).toBeChecked();
    expect(api.listScanRoots).toHaveBeenCalledTimes(1);
  });
});
