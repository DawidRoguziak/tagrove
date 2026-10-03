import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useLibraryKnownTags } from "../useLibraryKnownTags";

const apiMocks = vi.hoisted(() => ({ listTags: vi.fn() }));
vi.mock("../../api", async () => ({ ...(await vi.importActual<typeof import("../../api")>("../../api")), ...apiMocks }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, no) => { resolve = ok; reject = no; });
  return { promise, resolve, reject };
}

describe("useLibraryKnownTags", () => {
  beforeEach(() => apiMocks.listTags.mockReset());

  it("loads all pages in backend-sized chunks", async () => {
    apiMocks.listTags
      .mockResolvedValueOnce({ items: Array.from({ length: 200 }, (_, i) => `tag-${i}`), total: 201 })
      .mockResolvedValueOnce({ items: ["tag-200"], total: 201 });
    const { result } = renderHook(() => useLibraryKnownTags());
    await act(() => result.current.refreshKnownTags());
    expect(apiMocks.listTags).toHaveBeenNthCalledWith(2, { query: "", offset: 200, limit: 200 });
    expect(result.current.knownTags).toHaveLength(201);
  });

  it("does not publish a partial snapshot when a later page fails", async () => {
    apiMocks.listTags
      .mockResolvedValueOnce({ items: Array.from({ length: 200 }, (_, i) => `tag-${i}`), total: 201 })
      .mockRejectedValueOnce(new Error("page failed"));
    const { result } = renderHook(() => useLibraryKnownTags());
    act(() => result.current.setKnownTags(["confirmed"]));
    await expect(result.current.refreshKnownTags()).rejects.toThrow("page failed");
    expect(result.current.knownTags).toEqual(["confirmed"]);
  });

  it("ignores an older refresh that resolves after a newer one", async () => {
    const old = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(old.promise).mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    let oldPromise!: Promise<string[]>;
    act(() => { oldPromise = result.current.refreshKnownTags(); });
    let newer!: Promise<string[]>;
    act(() => { newer = result.current.refreshKnownTags(); });
    expect(newer).toBe(oldPromise);
    old.resolve({ items: ["old"], total: 1 });
    await act(() => oldPromise);
    await waitFor(() => expect(result.current.knownTags).toEqual(["new"]));
  });
  it("does not repopulate known tags when reset interrupts pagination", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result } = renderHook(() => useLibraryKnownTags());
    let refresh!: Promise<string[]>;
    act(() => { refresh = result.current.refreshKnownTags(); result.current.setKnownTags([]); });
    await act(async () => { page.resolve({ items: ["old"], total: 400 }); await refresh; });
    expect(result.current.knownTags).toEqual([]);
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
  });

});

describe("mutation-aware vocabulary refresh", () => {
  beforeEach(() => apiMocks.listTags.mockReset());

  it("does not fetch on repeated existing-vocabulary saves after hydration", async () => {
    apiMocks.listTags.mockResolvedValue({ items: ["cat", "dog"], total: 2 });
    const { result } = renderHook(() => useLibraryKnownTags());
    await act(() => result.current.hydrateKnownTags());
    const original = result.current.knownTags;
    for (let i = 0; i < 20; i++) {
      await act(() => result.current.refreshKnownTagsAfterMutation(false));
    }
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
    expect(result.current.knownTags).toBe(original);
    await act(() => result.current.refreshKnownTagsAfterMutation(true));
    expect(apiMocks.listTags).toHaveBeenCalledTimes(2);
    expect(result.current.knownTags).toBe(original);
    apiMocks.listTags.mockResolvedValue({ items: ["cat", "new"], total: 2 });
    await act(() => result.current.refreshKnownTagsAfterMutation(true));
    expect(result.current.knownTags).toEqual(["cat", "new"]);
    expect(result.current.knownTags).not.toBe(original);
  });

  it("compares ordered values and length rather than sets or prefixes", async () => {
    apiMocks.listTags.mockResolvedValueOnce({ items: ["a", "b"], total: 2 })
      .mockResolvedValueOnce({ items: ["b", "a"], total: 2 })
      .mockResolvedValueOnce({ items: ["b"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    await act(() => result.current.hydrateKnownTags());
    const first = result.current.knownTags;
    await act(() => result.current.refreshKnownTagsAfterMutation(true));
    expect(result.current.knownTags).not.toBe(first);
    const second = result.current.knownTags;
    await act(() => result.current.refreshKnownTagsAfterMutation(true));
    expect(result.current.knownTags).not.toBe(second);
    expect(result.current.knownTags).toEqual(["b"]);
  });

  it("joins an active refresh for unchanged saves without requesting another pass", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result } = renderHook(() => useLibraryKnownTags());
    const changing = result.current.refreshKnownTagsAfterMutation(true);
    expect(result.current.refreshKnownTagsAfterMutation(false)).toBe(changing);
    await act(async () => { page.resolve({ items: ["cat"], total: 1 }); await changing; });
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
    expect(result.current.knownTags).toEqual(["cat"]);
  });

  it.each([false, true])("runs the latest mutation pass after a superseded read, failure=%s", async (fail) => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise).mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    const old = result.current.refreshKnownTagsAfterMutation(true);
    expect(result.current.refreshKnownTagsAfterMutation(true)).toBe(old);
    expect(result.current.refreshKnownTagsAfterMutation(false)).toBe(old);
    await act(async () => {
      if (fail) page.reject(new Error("superseded"));
      else page.resolve({ items: ["old"], total: 400 });
      await old;
    });
    expect(result.current.knownTags).toEqual(["new"]);
    expect(apiMocks.listTags).toHaveBeenCalledTimes(2);
    expect(apiMocks.listTags).toHaveBeenLastCalledWith({ query: "", offset: 0, limit: 200 });
  });

  it("retries failed refreshes on the next unchanged save", async () => {
    apiMocks.listTags.mockResolvedValueOnce({ items: ["old"], total: 1 })
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    await act(() => result.current.hydrateKnownTags());
    await expect(result.current.refreshKnownTagsAfterMutation(true)).rejects.toThrow("offline");
    expect(result.current.knownTags).toEqual(["old"]);
    await act(() => result.current.refreshKnownTagsAfterMutation(false));
    expect(result.current.knownTags).toEqual(["new"]);
    expect(apiMocks.listTags).toHaveBeenCalledTimes(3);
  });

  it("hydrates on an unchanged save after startup failure", async () => {
    apiMocks.listTags.mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    await act(() => result.current.hydrateKnownTags());
    await act(() => result.current.refreshKnownTagsAfterMutation(false));
    expect(result.current.knownTags).toEqual(["new"]);
  });

  it("reset permits a fresh pass and an obsolete completion cannot clear its dirty state", async () => {
    const old = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(old.promise).mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["current"], total: 1 });
    const { result } = renderHook(() => useLibraryKnownTags());
    const pending = result.current.refreshKnownTagsAfterMutation(true);
    act(() => result.current.setKnownTags([]));
    await expect(result.current.refreshKnownTagsAfterMutation(false)).rejects.toThrow("offline");
    await act(async () => { old.resolve({ items: ["stale"], total: 400 }); await pending; });
    expect(result.current.knownTags).toEqual([]);
    await act(() => result.current.refreshKnownTagsAfterMutation(false));
    expect(result.current.knownTags).toEqual(["current"]);
  });

  it("stops pagination and publication after unmount", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result, unmount } = renderHook(() => useLibraryKnownTags());
    const pending = result.current.refreshKnownTagsAfterMutation(true);
    unmount();
    page.resolve({ items: ["stale"], total: 400 });
    await expect(pending).resolves.toEqual([]);
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
  });
});
