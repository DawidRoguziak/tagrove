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
