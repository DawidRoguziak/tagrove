import { renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { expect, it, vi } from "vitest";
import { createTagList } from "../createTagList";
import type { TagListPage } from "../../../../types";
const api = vi.hoisted(() => ({ listTags: vi.fn() }));
vi.mock("../../../../platform/tauri/tags", () => api);
it("rejects old queries and finalizers, deduplicates same-turn page requests and stops on close", async () => {
  const pending: ((page: TagListPage) => void)[] = [];
  api.listTags
    .mockReset()
    .mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
  const { result } = renderHook(() => {
    const [query, setQuery] = createSignal("old"),
      [open, setOpen] = createSignal(true);
    const model = createTagList({ open, query, tags: () => ["old", "new"], list: () => undefined });
    return { model, setQuery, setOpen };
  });
  flush();
  result.setQuery("new");
  flush();
  pending[0]({ items: ["obsolete"], total: 1 });
  await Promise.resolve();
  await Promise.resolve();
  flush();
  expect(result.model.page.items).not.toContain("obsolete");
  expect(result.model.loading).toBe(true);
  pending[1]({ items: ["new"], total: 200 });
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  flush();
  const a = result.model.loadMore(),
    b = result.model.loadMore();
  expect(api.listTags).toHaveBeenCalledTimes(3);
  result.setOpen(false);
  flush();
  pending[2]({ items: ["late"], total: 200 });
  await Promise.all([a, b]);
  flush();
  expect(result.model.page.items).toEqual([]);
  expect(result.model.loadingMore).toBe(false);
});

it("stops automatic pagination when a failed page has no fallback entries, but permits an explicit retry", async () => {
  api.listTags
    .mockReset()
    .mockResolvedValueOnce({ items: ["one"], total: 200 })
    .mockRejectedValueOnce(new Error("offline"))
    .mockImplementation(() => new Promise(() => {}));
  const list = document.createElement("div"),
    tags = ["one"];
  const { result } = renderHook(() =>
    createTagList({ open: () => true, query: () => "", tags: () => tags, list: () => list })
  );
  for (let turn = 0; turn < 10; turn++) {
    await Promise.resolve();
    flush();
  }
  expect(api.listTags).toHaveBeenCalledTimes(2);
  api.listTags.mockResolvedValueOnce({ items: ["two"], total: 2 });
  await result.loadMore();
  flush();
  expect(result.page.items).toEqual(["one", "two"]);
});
