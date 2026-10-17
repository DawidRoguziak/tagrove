import { renderHook, waitFor } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { createSuggestions } from "../createSuggestions";
import { SuggestionClient } from "../SuggestionClient";
import { readActiveToken } from "../tokenService";
import type { TagSuggestion } from "../types";
afterEach(() => vi.restoreAllMocks());
it("rejects obsolete results, stops blurred reads and cancels on disposal", async () => {
  const pending: ((items: TagSuggestion[]) => void)[] = [];
  const client = new SuggestionClient();
  const search = vi
    .spyOn(client, "search")
    .mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
  const cancel = vi.spyOn(client, "cancel");
  const tags = ["cat", "dog"],
    excluded: string[] = [],
    used = new Set<string>();
  const { result, cleanup } = renderHook(() => {
    const [value, setValue] = createSignal("ca"),
      [focused, setFocused] = createSignal(true);
    const model = createSuggestions(client, {
      tags: () => tags,
      token: () => readActiveToken(value(), value().length),
      used: () => used,
      eligible: focused,
      excluded: () => excluded
    });
    return { model, setValue, setFocused };
  });
  flush();
  expect(search).toHaveBeenCalledOnce();
  result.setValue("ca");
  flush();
  expect(search).toHaveBeenCalledOnce();
  result.setValue("do");
  flush();
  pending[1]([{ value: "dog", indices: [] }]);
  await Promise.resolve();
  flush();
  pending[0]([{ value: "cat", indices: [] }]);
  await Promise.resolve();
  flush();
  expect(result.model.items[0]?.value).toBe("dog");
  result.setFocused(false);
  flush();
  expect(result.model.items).toEqual([]);
  expect(search).toHaveBeenCalledTimes(2);
  cleanup();
  expect(cancel).toHaveBeenCalled();
});
it("allows explicit retry after worker failure", async () => {
  const client = new SuggestionClient();
  vi.spyOn(client, "search")
    .mockRejectedValueOnce(new Error("worker"))
    .mockResolvedValueOnce([{ value: "cat", indices: [] }]);
  const tags = ["cat"];
  const { result } = renderHook(() =>
    createSuggestions(client, {
      tags: () => tags,
      token: () => readActiveToken("ca", 2),
      used: () => new Set(),
      eligible: () => true,
      excluded: () => []
    })
  );
  await waitFor(() => expect(result.failed).toBe(true));
  result.retry();
  flush();
  await waitFor(() => expect(result.items[0]?.value).toBe("cat"));
});

it("retains failure across blur but invalidates it when the draft or vocabulary changes", async () => {
  const client = new SuggestionClient();
  const search = vi.spyOn(client, "search").mockRejectedValue(new Error("worker"));
  const excluded: string[] = [];
  const { result, cleanup } = renderHook(() => {
    const [focused, setFocused] = createSignal(true);
    const [value, setValue] = createSignal("ca");
    const [tags, setTags] = createSignal(["cat"]);
    const model = createSuggestions(client, {
      tags, token: () => readActiveToken(value(), value().length),
      used: () => new Set<string>(), eligible: focused, excluded: () => excluded
    });
    return { model, setFocused, setValue, setTags };
  });
  await waitFor(() => expect(result.model.failed).toBe(true));
  result.setFocused(false);
  flush();
  expect(result.model.failed).toBe(true);
  expect(search).toHaveBeenCalledTimes(1);
  result.setValue("");
  flush();
  expect(result.model.failed).toBe(false);
  result.setValue("ca");
  result.setFocused(true);
  flush();
  await waitFor(() => expect(search).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(result.model.failed).toBe(true));
  result.setFocused(false);
  result.setTags(["car"]);
  flush();
  expect(result.model.failed).toBe(false);
  cleanup();
  client.dispose();
});
