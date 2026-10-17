import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { createAppRuntime } from "../createAppRuntime";
import { LanguageProvider } from "../../i18n/solid";
import { settle } from "../../test/settle";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import type { ScanSummary } from "../../platform/tauri/contracts/scanning";

const native = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false, invoke: native.invoke, convertFileSrc: (value: string) => value
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: native.listen }));

function asset(id: number): AssetSummary {
  return {
    id, file_name: `${id}.png`, preview_path: null, kind: "image", modified_at: 1,
    width: 10, height: 10, duration_ms: null, thumb_path: null, is_favorite: false,
    media_group_key: null, media_group_order: null
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const summary: ScanSummary = { indexed: 4, removed: 0, failed: 0, completion: "complete" };
let rows: AssetSummary[];
let scan: ReturnType<typeof deferred<ScanSummary | null>>;
let session: number;
let completions: Promise<void>[];

function mount() {
  const owner = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  const start = owner.result.settings.scan.scanOnStartup;
  vi.spyOn(owner.result.settings.scan, "scanOnStartup").mockImplementation(() => {
    const completion = start();
    completions.push(completion);
    return completion;
  });
  settle(() => { owner.result.start(); owner.result.start(); });
  return owner;
}
function calls(command: string) {
  return native.invoke.mock.calls.filter(([name]) => name === command).length;
}
beforeEach(() => {
  vi.resetAllMocks();
  native.listen.mockImplementation(async () => vi.fn());
  rows = [];
  session = 0;
  completions = [];
  scan = deferred();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "scan_startup_roots") return scan.promise;
    if (command === "start_asset_query") return {
      status: "ready", session_id: ++session, revision: session, total: rows.length,
      offset: 0, items: rows.slice(0, 128)
    };
    if (command === "list_tags") return { items: [], total: 0 };
    if (command === "list_scan_roots" || command === "get_startup_popular_tags") return [];
    if (command === "get_video_tool_status") return { ffmpeg_available: true, ffprobe_available: true };
  });
});

it.each([0, 3, 160])("refreshes the replacement with %i initial rows after shared scan completion", async (count) => {
  rows = Array.from({ length: count }, (_, index) => asset(index + 1));
  const first = mount();
  await waitFor(() => expect(calls("scan_startup_roots")).toBe(1));
  first.cleanup();
  const replacement = mount();
  await waitFor(() => expect(calls("scan_startup_roots")).toBe(2));
  await waitFor(() => {
    expect(replacement.result.library.query.queryEpoch).toBeGreaterThan(0);
    expect(replacement.result.library.query.loading).toBe(false);
    expect(replacement.result.library.query.total).toBe(count);
  });
  expect(calls("start_asset_query")).toBe(2);
  rows = [...rows, asset(count + 1)];
  scan.resolve(summary);
  await settle(() => Promise.all(completions));
  expect(replacement.result.library.query.total).toBe(count + 1);
  expect(calls("start_asset_query")).toBe(3);
  expect(calls("list_scan_roots")).toBe(3);
  expect(replacement.result.settings.scan.operationState.message).toContain("Indexed: 4");
  expect(replacement.result.maintenanceLoading()).toBe(false);
  // No page demand or progress event was delivered to trigger an incidental refresh.
  expect(calls("get_asset_query_page")).toBe(0);
  expect(calls("scan_startup_roots")).toBe(2);
});

it("replays completion on a later mount and skips refresh when the launch had no work", async () => {
  scan.resolve(summary);
  const first = mount();
  await settle(() => Promise.all(completions));
  first.cleanup();
  const replacement = mount();
  await settle(() => Promise.all(completions));
  expect(replacement.result.settings.scan.operationState.message).toContain("Indexed: 4");
  expect(calls("start_asset_query")).toBe(4);
  replacement.cleanup();
  scan = deferred();
  scan.resolve(null);
  mount();
  await settle(() => Promise.all(completions));
  expect(calls("start_asset_query")).toBe(5);
});

it("refreshes committed rows and exposes a shared failure only in the replacement", async () => {
  const first = mount();
  await waitFor(() => expect(calls("scan_startup_roots")).toBe(1));
  first.cleanup();
  const replacement = mount();
  await waitFor(() => expect(calls("scan_startup_roots")).toBe(2));
  rows = [asset(1)];
  scan.reject(new Error("synthetic scan failure"));
  await settle(() => Promise.all(completions));
  expect(replacement.result.library.query.total).toBe(1);
  expect(replacement.result.settings.scan.operationState.message).toContain("synthetic scan failure");
  expect(calls("start_asset_query")).toBe(3);
  expect(calls("list_scan_roots")).toBe(3);
  await replacement.result.settings.scan.scanOnStartup();
  expect(calls("scan_startup_roots")).toBe(2);
});
