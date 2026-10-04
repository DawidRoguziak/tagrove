import { settle } from "../../../test/settle";
import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createKnownTags } from "../createKnownTags";

const apiMocks = vi.hoisted(() => ({ listTags: vi.fn() }));
vi.mock("../../../platform/tauri/tags", async () => ({
  ...(await vi.importActual<typeof import("../../../platform/tauri/tags")>(
    "../../../platform/tauri/tags"
  )),
  ...apiMocks
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
}

describe("createKnownTags", () => {
  beforeEach(() => apiMocks.listTags.mockReset());

  it("loads all pages in backend-sized chunks", async () => {
    apiMocks.listTags
      .mockResolvedValueOnce({
        items: Array.from({ length: 200 }, (_, i) => `tag-${i}`),
        total: 201
      })
      .mockResolvedValueOnce({ items: ["tag-200"], total: 201 });
    const { result } = renderHook(() => createKnownTags());
    await settle(() => result.refreshKnownTags());
    expect(apiMocks.listTags).toHaveBeenNthCalledWith(2, { query: "", offset: 200, limit: 200 });
    expect(result.knownTags).toHaveLength(201);
  });

  it("does not publish a partial snapshot when a later page fails", async () => {
    const { result } = renderHook(() => createKnownTags());
    apiMocks.listTags.mockResolvedValueOnce({ items: ["confirmed"], total: 1 });
    await settle(() => result.hydrateKnownTags());
    apiMocks.listTags
      .mockResolvedValueOnce({
        items: Array.from({ length: 200 }, (_, i) => `tag-${i}`),
        total: 201
      })
      .mockRejectedValueOnce(new Error("page failed"));
    await expect(result.refreshKnownTags()).rejects.toThrow("page failed");
    expect(result.knownTags).toEqual(["confirmed"]);
  });

  it("ignores an older refresh that resolves after a newer one", async () => {
    const old = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags
      .mockReturnValueOnce(old.promise)
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    let oldPromise!: Promise<string[]>;
    settle(() => {
      oldPromise = result.refreshKnownTags();
    });
    let newer!: Promise<string[]>;
    settle(() => {
      newer = result.refreshKnownTags();
    });
    expect(newer).toBe(oldPromise);
    old.resolve({ items: ["old"], total: 1 });
    await settle(() => oldPromise);
    await waitFor(() => expect(result.knownTags).toEqual(["new"]));
  });
  it("does not repopulate known tags when reset interrupts pagination", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result } = renderHook(() => createKnownTags());
    let refresh!: Promise<string[]>;
    settle(() => {
      refresh = result.refreshKnownTags();
      result.reset();
    });
    await settle(async () => {
      page.resolve({ items: ["old"], total: 400 });
      await refresh;
    });
    expect(result.knownTags).toEqual([]);
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
  });
});

describe("mutation-aware vocabulary refresh", () => {
  beforeEach(() => apiMocks.listTags.mockReset());

  it("does not fetch on repeated existing-vocabulary saves after hydration", async () => {
    apiMocks.listTags.mockResolvedValue({ items: ["cat", "dog"], total: 2 });
    const { result } = renderHook(() => createKnownTags());
    await settle(() => result.hydrateKnownTags());
    const original = result.knownTags;
    for (let i = 0; i < 20; i++) {
      await settle(() => result.refreshKnownTagsAfterMutation(false));
    }
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
    expect(result.knownTags).toBe(original);
    await settle(() => result.refreshKnownTagsAfterMutation(true));
    expect(apiMocks.listTags).toHaveBeenCalledTimes(2);
    expect(result.knownTags).toBe(original);
    apiMocks.listTags.mockResolvedValue({ items: ["cat", "new"], total: 2 });
    await settle(() => result.refreshKnownTagsAfterMutation(true));
    expect(result.knownTags).toEqual(["cat", "new"]);
    expect(result.knownTags).not.toBe(original);
  });

  it("compares ordered values and length rather than sets or prefixes", async () => {
    apiMocks.listTags
      .mockResolvedValueOnce({ items: ["a", "b"], total: 2 })
      .mockResolvedValueOnce({ items: ["b", "a"], total: 2 })
      .mockResolvedValueOnce({ items: ["b"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    await settle(() => result.hydrateKnownTags());
    const first = result.knownTags;
    await settle(() => result.refreshKnownTagsAfterMutation(true));
    expect(result.knownTags).not.toBe(first);
    const second = result.knownTags;
    await settle(() => result.refreshKnownTagsAfterMutation(true));
    expect(result.knownTags).not.toBe(second);
    expect(result.knownTags).toEqual(["b"]);
  });

  it("joins an active refresh for unchanged saves without requesting another pass", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result } = renderHook(() => createKnownTags());
    const changing = result.refreshKnownTagsAfterMutation(true);
    expect(result.refreshKnownTagsAfterMutation(false)).toBe(changing);
    await settle(async () => {
      page.resolve({ items: ["cat"], total: 1 });
      await changing;
    });
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
    expect(result.knownTags).toEqual(["cat"]);
  });

  it.each([
    false,
    true
  ])("runs the latest mutation pass after a superseded read, failure=%s", async (fail) => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags
      .mockReturnValueOnce(page.promise)
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    const old = result.refreshKnownTagsAfterMutation(true);
    expect(result.refreshKnownTagsAfterMutation(true)).toBe(old);
    expect(result.refreshKnownTagsAfterMutation(false)).toBe(old);
    await settle(async () => {
      if (fail) page.reject(new Error("superseded"));
      else page.resolve({ items: ["old"], total: 400 });
      await old;
    });
    expect(result.knownTags).toEqual(["new"]);
    expect(apiMocks.listTags).toHaveBeenCalledTimes(2);
    expect(apiMocks.listTags).toHaveBeenLastCalledWith({ query: "", offset: 0, limit: 200 });
  });

  it("retries failed refreshes on the next unchanged save", async () => {
    apiMocks.listTags
      .mockResolvedValueOnce({ items: ["old"], total: 1 })
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    await settle(() => result.hydrateKnownTags());
    await expect(result.refreshKnownTagsAfterMutation(true)).rejects.toThrow("offline");
    expect(result.knownTags).toEqual(["old"]);
    await settle(() => result.refreshKnownTagsAfterMutation(false));
    expect(result.knownTags).toEqual(["new"]);
    expect(apiMocks.listTags).toHaveBeenCalledTimes(3);
  });

  it("hydrates on an unchanged save after startup failure", async () => {
    apiMocks.listTags
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["new"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    await settle(() => result.hydrateKnownTags());
    await settle(() => result.refreshKnownTagsAfterMutation(false));
    expect(result.knownTags).toEqual(["new"]);
  });

  it("reset permits a fresh pass and an obsolete completion cannot clear its dirty state", async () => {
    const old = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags
      .mockReturnValueOnce(old.promise)
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ items: ["current"], total: 1 });
    const { result } = renderHook(() => createKnownTags());
    const pending = result.refreshKnownTagsAfterMutation(true);
    settle(() => result.reset());
    await expect(result.refreshKnownTagsAfterMutation(false)).rejects.toThrow("offline");
    await settle(async () => {
      old.resolve({ items: ["stale"], total: 400 });
      await pending;
    });
    expect(result.knownTags).toEqual([]);
    await settle(() => result.refreshKnownTagsAfterMutation(false));
    expect(result.knownTags).toEqual(["current"]);
  });

  it("stops pagination and publication after unmount", async () => {
    const page = deferred<{ items: string[]; total: number }>();
    apiMocks.listTags.mockReturnValueOnce(page.promise);
    const { result, cleanup: unmount } = renderHook(() => createKnownTags());
    const pending = result.refreshKnownTagsAfterMutation(true);
    unmount();
    page.resolve({ items: ["stale"], total: 400 });
    await expect(pending).resolves.toEqual([]);
    expect(apiMocks.listTags).toHaveBeenCalledOnce();
  });
});
