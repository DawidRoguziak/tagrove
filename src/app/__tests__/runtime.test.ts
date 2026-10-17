import { renderHook, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createAppRuntime } from "../createAppRuntime";
import { LanguageProvider } from "../../i18n/solid";
import { settle } from "../../test/settle";
import * as tagsApi from "../../platform/tauri/tags";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: native.invoke,
  convertFileSrc: (value: string) => value
}));

beforeEach(() => {
  native.invoke.mockReset();
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "start_asset_query")
      return { status: "ready", session_id: 1, revision: 1, total: 0, offset: 0, items: [] };
    if (command === "list_tags") return { items: [], total: 0 };
    if (command === "list_scan_roots") return [];
    if (command === "get_startup_popular_tags") return ["travel", "cat"];
  });
});
afterEach(() => vi.restoreAllMocks());

it("loads the startup snapshot once and retains it across filters and view changes", async () => {
  const { result } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  await settle(() => { result.start(); result.start(); });
  await waitFor(() => expect(result.popularTags()).toEqual(["travel", "cat"]));
  await settle(() => result.search.applyInput("travel"));
  await settle(() => result.openSettings());
  await settle(() => result.closeSettings());
  result.start();
  expect(result.popularTags()).toEqual(["travel", "cat"]);
  expect(native.invoke.mock.calls.filter(([command]) => command === "get_startup_popular_tags")).toHaveLength(1);
});

it("logs a failed snapshot and leaves the shared library usable", async () => {
  const error = new Error("snapshot unavailable");
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  const invoke = native.invoke.getMockImplementation();
  native.invoke.mockImplementation((command: string, args: unknown) =>
    command === "get_startup_popular_tags" ? Promise.reject(error) : invoke?.(command, args)
  );
  const { result } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  await settle(() => result.start());
  await waitFor(() => expect(log).toHaveBeenCalledWith("Failed to load startup popular tags", error));
  expect(result.popularTags()).toEqual([]);
  await settle(() => result.search.applyInput("cat"));
  expect(native.invoke).toHaveBeenCalledWith("start_asset_query", expect.objectContaining({ tagsAnd: ["cat"] }));
});

it("ignores a startup snapshot that arrives after disposal", async () => {
  const getStartupPopularTags = tagsApi.getStartupPopularTags;
  const handlerCompletions: Promise<unknown>[] = [];
  vi.spyOn(tagsApi, "getStartupPopularTags").mockImplementation(() => {
    const response = getStartupPopularTags();
    const then = response.then.bind(response);
    // Keep the real adapter and capture completion of the runtime's consumer.
    vi.spyOn(response, "then").mockImplementation((onFulfilled, onRejected) => {
      const completion = then(onFulfilled, onRejected);
      // The runtime registers success here and rejection on the chained catch.
      // Vitest also observes the promise, but supplies both callbacks.
      if (onFulfilled && !onRejected) handlerCompletions.push(completion);
      return completion;
    });
    return response;
  });
  let resolve!: (tags: string[]) => void;
  const pending = new Promise<string[]>((done) => { resolve = done; });
  const invoke = native.invoke.getMockImplementation();
  native.invoke.mockImplementation((command: string, args: unknown) =>
    command === "get_startup_popular_tags" ? pending : invoke?.(command, args)
  );
  const { result, cleanup } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  await settle(() => result.start());
  cleanup();
  expect(handlerCompletions).toHaveLength(1);
  resolve(["late"]);
  await settle(() => Promise.all(handlerCompletions));
  expect(result.popularTags()).toEqual([]);
});
