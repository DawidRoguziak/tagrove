import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { createAppRuntime } from "../createAppRuntime";
import { LanguageProvider } from "../../i18n/solid";
import { settle } from "../../test/settle";
import { asset, summary, deferred } from "../../features/lightbox/__tests__/fixtures";
import type { SetAssetTagsSummary } from "../../platform/tauri/contracts/tags";
import type { ScanSummary } from "../../platform/tauri/contracts/scanning";
import type { DuplicateScanSummary, DuplicateResolutionBatchSummary } from "../../platform/tauri/contracts/maintenance";

const api = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false, invoke: api.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: api.open, save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
let persisted: string[];
let fileName: string;
const saved = (tags: string[]): SetAssetTagsSummary => ({ asset_id: 1, tags, changed: true,
  vocabulary_changed: false, query_impact: { type: "none" }, revision: 2 });

beforeEach(() => {
  persisted = ["cat"];
  fileName = "1.png";
  api.open.mockReset().mockResolvedValue("/synthetic/tags.csv");
  api.invoke.mockReset().mockImplementation(async (command: string, args: { tags?: string[] } = {}) => {
    if (command === "start_asset_query") return { status: "ready", session_id: 1, revision: 1, total: 1, offset: 0, items: [summary(1, { file_name: fileName })] };
    if (command === "get_asset_details") return asset(1, { tags: [...persisted], file_name: fileName });
    if (command === "get_asset_summaries_by_ids") return [summary(1)];
    if (command === "list_tags") return { items: ["cat"], total: 1 };
    if (command === "list_scan_roots") return [];
    if (command === "set_asset_tags") { persisted = args.tags ?? []; return saved(persisted); }
    if (command === "clear_library_data") return { removed_assets: 1, removed_roots: 0, removed_thumbnails: 0 };
    throw new Error(`Unexpected command: ${command}`);
  });
});

function calls(command: string) { return api.invoke.mock.calls.filter(([name]) => name === command); }
async function setup() {
  const { result } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  await settle(() => result.library.refreshLibrary());
  settle(() => result.lightbox.select(summary(1)));
  await waitFor(() => expect(result.lightbox.selected?.tags).toEqual(["cat"]));
  return result;
}
async function selectBulk(runtime: ReturnType<typeof createAppRuntime>) {
  settle(() => {
    runtime.selection.toggle();
    runtime.selection.interact({ type: "click", assetId: 1, assetIndex: 0, ctrlLike: false, shift: false });
  });
  await waitFor(() => expect(runtime.bulk.tagDetailsLoading).toBe(false));
}

it.each(["rename", "rescan"])("does not restore pre-%s details when a delayed tag save completes", async operation => {
  const runtime = await setup();
  const pending = deferred<SetAssetTagsSummary>();
  const original = api.invoke.getMockImplementation()!;
  let changed = false;
  api.invoke.mockImplementation((command: string, args: unknown) => {
    if (command === "set_asset_tags") return pending.promise;
    if (command === "get_asset_details") return Promise.resolve(asset(1, {
      tags: [...persisted], file_name: fileName,
      path: `/synthetic/${fileName}`, size_bytes: changed ? 99 : 10
    }));
    if (command === "find_duplicate_assets") return Promise.resolve({
      groups: [{ file_name: "1.png", assets: [{ id: 1, path: "/synthetic/1.png", size_bytes: 10, record_version: 1 }] }],
      duplicate_assets: 1, duplicate_groups: 1, revision: 1
    } satisfies DuplicateScanSummary);
    if (command === "apply_duplicate_resolution_batch") {
      changed = true;
      fileName = "renamed.png";
      return Promise.resolve({ status: "committed", results: [], revision: 2, removed_thumbnails: 0 } satisfies DuplicateResolutionBatchSummary);
    }
    if (command === "rescan_all_roots") {
      changed = true;
      return Promise.resolve({ completion: "complete", indexed: 1, removed: 0, failed: 0 } satisfies ScanSummary);
    }
    return original(command, args);
  });
  settle(() => runtime.lightbox.saveTags(["saved"]));
  expect(calls("set_asset_tags")).toHaveLength(1);
  settle(() => { runtime.lightbox.close(); runtime.openSettings(); });
  if (operation === "rename") {
    await settle(() => runtime.settings.duplicates.onStart());
    await settle(() => runtime.settings.duplicates.onSaveAll([
      { assetId: 1, type: "rename", nextFileName: "renamed.png" }
    ]));
    expect(calls("apply_duplicate_resolution_batch")).toHaveLength(1);
  } else {
    await settle(() => runtime.settings.scan.onRescanAll());
    expect(calls("rescan_all_roots")).toHaveLength(1);
  }
  expect(calls("start_asset_query")).toHaveLength(2);
  expect(runtime.library.details.get(1)).toBeNull();
  persisted = ["saved"];
  pending.resolve(saved(persisted));
  await waitFor(() => expect(runtime.library.tags.get(1)?.tags).toEqual(["saved"]));
  expect(runtime.library.details.get(1)).toBeNull();
  settle(() => { runtime.closeSettings(); runtime.lightbox.select(summary(1, { file_name: fileName })); });
  await waitFor(() => expect(runtime.lightbox.selected?.size_bytes).toBe(99));
  expect(runtime.lightbox.selected?.path).toBe(`/synthetic/${fileName}`);
  expect(runtime.lightbox.tagEditor).toEqual(["saved"]);
  expect(calls("get_asset_details")).toHaveLength(2);
});

it.each([false, true])("settings CSV drains pending edits and invalidates actual runtime caches, rejection=%s", async rejected => {
  const runtime = await setup();
  const pending = deferred<SetAssetTagsSummary>();
  const original = api.invoke.getMockImplementation()!;
  api.invoke.mockImplementation((command: string, args: unknown) => {
    if (command === "set_asset_tags" && calls(command).length === 1) return pending.promise;
    if (command === "import_tags_csv") {
      persisted = ["cat", "local", rejected ? "partial" : "imported"];
      return rejected ? Promise.reject(new Error("malformed later row")) : Promise.resolve({ rows_read: 1, rows_applied: 1, assets_matched: 1, assets_updated: 1 });
    }
    return original(command, args);
  });
  settle(() => runtime.lightbox.saveTags(["cat", "local"]));
  await waitFor(() => expect(calls("set_asset_tags")).toHaveLength(1));
  settle(() => { runtime.lightbox.close(); runtime.openSettings(); });
  const importing = runtime.settings.importExport.onImportCsv();
  await waitFor(() => expect(api.open).toHaveBeenCalledOnce());
  expect(calls("import_tags_csv")).toHaveLength(0);
  pending.resolve(saved(["cat", "local"]));
  await settle(() => importing);
  expect(calls("import_tags_csv")).toHaveLength(1);
  expect(runtime.library.details.get(1)).toBeNull();
  settle(() => { runtime.closeSettings(); runtime.lightbox.select(summary(1)); });
  await waitFor(() => expect(runtime.lightbox.selected?.tags).toEqual(persisted));
  expect(calls("get_asset_details")).toHaveLength(2);
  const next = [...persisted, "new"];
  settle(() => runtime.lightbox.saveTags(next));
  await waitFor(() => expect(api.invoke).toHaveBeenLastCalledWith("set_asset_tags", { assetId: 1, tags: next }));
});

it("clearing through settings discards the cached identity before an ID is reused", async () => {
  const runtime = await setup();
  const epoch = runtime.library.tags.epoch;
  settle(() => { runtime.lightbox.close(); runtime.openSettings(); runtime.settings.dangerZone.onOpenClearConfirm(); });
  await settle(() => runtime.settings.dangerZone.onConfirmClearYes());
  expect(calls("clear_library_data")).toHaveLength(1);
  expect(runtime.library.tags.epoch).toBeGreaterThan(epoch);
  expect(runtime.library.details.get(1)).toBeNull();
  persisted = ["reused"];
  fileName = "reused.png";
  await settle(() => runtime.library.refreshLibrary());
  settle(() => { runtime.closeSettings(); runtime.lightbox.select(summary(1, { file_name: fileName })); });
  await waitFor(() => expect(runtime.lightbox.selected?.tags).toEqual(["reused"]));
  expect(runtime.lightbox.selected?.file_name).toBe("reused.png");
  expect(calls("get_asset_details")).toHaveLength(2);
});

it.each(["lightbox", "bulk"] as const)("shares locks and canonical tags when %s writes first", async first => {
  const runtime = await setup();
  await selectBulk(runtime);
  const pending = deferred<SetAssetTagsSummary>();
  const original = api.invoke.getMockImplementation()!;
  api.invoke.mockImplementation((command: string, args: unknown) => command === "set_asset_tags" && calls(command).length === 1 ? pending.promise : original(command, args));
  let bulkWrite: Promise<boolean> | undefined;
  if (first === "lightbox") settle(() => runtime.lightbox.saveTags(["cat", "dog"]));
  else bulkWrite = runtime.bulk.onAddTag("dog");
  await waitFor(() => expect(calls("set_asset_tags")).toHaveLength(1));
  if (first === "lightbox") expect(await runtime.bulk.onAddTag("bird")).toBe(false);
  else settle(() => runtime.lightbox.saveTags(["cat", "bird"]));
  await settle(async () => { await Promise.resolve(); });
  expect(calls("set_asset_tags")).toHaveLength(1);
  persisted = ["cat", "dog"];
  pending.resolve(saved(persisted));
  if (bulkWrite) await settle(() => bulkWrite!);
  await waitFor(() => expect(runtime.bulk.singleAssetTags).toEqual(["cat", "dog"]));
  if (first === "lightbox") expect(await settle(() => runtime.bulk.onAddTag("bird"))).toBe(true);
  else settle(() => runtime.lightbox.retryTags());
  await waitFor(() => expect(calls("set_asset_tags")).toHaveLength(2));
  expect(calls("set_asset_tags")[1]).toEqual(["set_asset_tags", { assetId: 1, tags: ["cat", "dog", "bird"] }]);
  await waitFor(() => expect(runtime.lightbox.selected?.tags).toEqual(["cat", "dog", "bird"]));
});
