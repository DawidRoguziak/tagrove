import { screen, within, waitFor } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { createSettings } from "../createSettings";
import { SettingsPanel } from "../SettingsPanel";
import { renderSettings } from "./renderSettings";
import { settle } from "../../../test/settle";
const api = vi.hoisted(() => ({
  getVideoToolStatus: vi.fn(),
  listScanRoots: vi.fn(),
  open: vi.fn(),
  inspectDbBundle: vi.fn()
}));
vi.mock("../../../platform/tauri/maintenance", () => api);
vi.mock("../../../platform/tauri/scanning", () => api);
vi.mock("../../../platform/tauri/dialogs", () => api);
beforeEach(() => {
  vi.resetAllMocks();
  api.getVideoToolStatus.mockResolvedValue(null);
  api.listScanRoots.mockResolvedValue([{ path: "/synthetic/media", auto_scan_on_startup: false }]);
  api.open.mockResolvedValue("/synthetic/backup.zip");
  api.inspectDbBundle.mockResolvedValue({ requires_mapping: false, roots: [] });
});
function setup(stubActions = false) {
  let model!: ReturnType<typeof createSettings>;
  const language = vi.fn(),
    theme = vi.fn();
  function Panel() {
    model = createSettings({
      setLoading: vi.fn(),
      refreshLibrary: vi.fn(async () => {}),
      onRootRemoved: vi.fn(),
      onImportDbRestored: vi.fn(),
      onLibraryCleared: vi.fn(),
      onTagCacheInvalidated: vi.fn(),
      runWithTagMutationBarrier: (operation) => operation()
    });
    vi.spyOn(model.dangerZone, "onConfirmClearYes").mockResolvedValue();
    vi.spyOn(model.scan, "onConfirmRemoveRoot").mockResolvedValue();
    if (stubActions) {
      for (const key of [
        "onPickFolder",
        "onRescanAll",
        "onRescanRoot",
        "onRenderAllThumbnails",
        "onRenderFailedThumbnails"
      ] as const)
        vi.spyOn(model.scan, key).mockResolvedValue();
      for (const key of [
        "onExportCsv",
        "onImportCsv",
        "onExportDbBundle",
        "onImportDbBundle"
      ] as const)
        vi.spyOn(model.importExport, key).mockResolvedValue();
      vi.spyOn(model.scan, "onRequestRemoveRoot").mockImplementation(() => {});
      vi.spyOn(model.dangerZone, "onOpenClearConfirm").mockImplementation(() => {});
      vi.spyOn(model.duplicates, "onStart").mockResolvedValue();
    }
    return (
      <SettingsPanel
        {...model}
        highlightScanSection={false}
        appearance={{
          theme: "dark",
          onThemeChange: theme,
          language: "en",
          onLanguageChange: language
        }}
      />
    );
  }
  const view = renderSettings(() => (
    <main>
      <Panel />
    </main>
  ));
  return { ...view, model, language, theme };
}
it("renders all sections in order and navigates by focus without changing the URL", async () => {
  const view = setup();
  for (const heading of [
    "Scan settings",
    "Appearance",
    "Language",
    "Import / Export",
    "Duplicate manager",
    "Danger zone",
    "About"
  ])
    expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
  const navigation = screen.getByRole("navigation", { name: "Settings sections" });
  const hash = window.location.hash;
  for (const [id, name] of [
    ["appearance", "Appearance"],
    ["language", "Language"],
    ["about", "About"]
  ]) {
    const section = view.container.querySelector<HTMLElement>(`#settings-${id}`);
    if (!section) throw new Error("Missing section");
    const scroll = vi.fn();
    section.scrollIntoView = scroll;
    const link = within(navigation).getByRole("link", { name });
    await userEvent.click(link);
    expect(section).toHaveFocus();
    expect(scroll).toHaveBeenCalledWith({ block: "start", behavior: "instant" });
    expect(link).toHaveAttribute("aria-current", "location");
    expect(window.location.hash).toBe(hash);
  }
  expect(document.getElementById("settings-appearance")?.nextElementSibling).toBe(
    document.getElementById("settings-language")
  );
});
it("wires section actions to their application models and preference callbacks", async () => {
  const { model, language } = setup(true);
  await settle(() => model.scan.refreshScanRoots());
  const calls = [
    ["Choose folder", vi.mocked(model.scan.onPickFolder)],
    ["Rescan all", vi.mocked(model.scan.onRescanAll)],
    ["Rescan", vi.mocked(model.scan.onRescanRoot)],
    ["Render thumbnails for all indexed assets", vi.mocked(model.scan.onRenderAllThumbnails)],
    ["Retry failed thumbnails only", vi.mocked(model.scan.onRenderFailedThumbnails)],
    ["Export tags CSV", vi.mocked(model.importExport.onExportCsv)],
    ["Import tags CSV", vi.mocked(model.importExport.onImportCsv)],
    ["Export DB + thumbnails", vi.mocked(model.importExport.onExportDbBundle)],
    ["Import DB + thumbnails", vi.mocked(model.importExport.onImportDbBundle)],
    [
      "Remove all thumbnails, indexed assets and scan paths",
      vi.mocked(model.dangerZone.onOpenClearConfirm)
    ],
    ["Start", vi.mocked(model.duplicates.onStart)],
    ["Remove", vi.mocked(model.scan.onRequestRemoveRoot)]
  ] as const;
  await userEvent.selectOptions(screen.getByRole("combobox", { name: "Select language" }), "pl");
  expect(language).toHaveBeenCalledWith("pl");
  for (const [name, spy] of calls) {
    await userEvent.click(screen.getByRole("button", { name }));
    expect(spy).toHaveBeenCalledTimes(1);
  }
  expect(model.scan.onRescanRoot).toHaveBeenCalledWith("/synthetic/media");
  expect(model.scan.onRequestRemoveRoot).toHaveBeenCalledWith("/synthetic/media");
});
it("wires clear and root confirmation callbacks", async () => {
  const { model } = setup();
  settle(model.dangerZone.onOpenClearConfirm);
  const clear = vi.mocked(model.dangerZone.onConfirmClearYes);
  await userEvent.click(screen.getByRole("button", { name: "YES" }));
  expect(clear).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole("button", { name: "No" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  settle(() => model.scan.onRequestRemoveRoot("/synthetic/media"));
  const remove = vi.mocked(model.scan.onConfirmRemoveRoot);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Remove scan path?" })).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(remove).toHaveBeenCalledTimes(1);
  await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(model.scan.removeRootConfirmPath).toBeNull();
});
it("uses the canonical Confirm label for duplicate deletion and database overwrite", async () => {
  const { model } = setup();
  await settle(() => model.duplicates.onSaveAll([{ assetId: 1, type: "delete" }]));
  let dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Confirm" })).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await settle(() => model.importExport.onImportDbBundle());
  dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading", { name: "Confirm" })).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeInTheDocument();
});
