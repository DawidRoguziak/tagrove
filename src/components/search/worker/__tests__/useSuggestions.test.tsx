import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useSuggestions } from "../SuggestionProvider";
import { SuggestionClient } from "../SuggestionClient";
import { readActiveToken } from "../../services/tokenService";
import type { TagSuggestion } from "../../types";
afterEach(() => vi.restoreAllMocks());
it("rejects obsolete results and does not search on unrelated thumbnail renders or blurred inputs", async () => {
  const pending: Array<(items: TagSuggestion[]) => void> = [];
  const search = vi
    .spyOn(SuggestionClient.prototype, "search")
    .mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
  const tags = ["cat", "dog"];
  const { result, rerender, unmount } = renderHook(
    ({ value, focused }) =>
      useSuggestions(tags, readActiveToken(value, value.length), new Set(), focused, []),
    { initialProps: { value: "ca", focused: true } }
  );
  expect(search).toHaveBeenCalledOnce();
  rerender({ value: "ca", focused: true });
  expect(search).toHaveBeenCalledOnce();
  rerender({ value: "do", focused: true });
  await act(async () => pending[1]!([{ value: "dog", indices: [] }]));
  await act(async () => pending[0]!([{ value: "cat", indices: [] }]));
  expect(result.current.suggestions[0]?.value).toBe("dog");
  rerender({ value: "do", focused: false });
  expect(result.current.suggestions).toEqual([]);
  expect(search).toHaveBeenCalledTimes(2);
  unmount();
});
it("offers explicit suggestion recovery after worker failure", async () => {
  const search = vi
    .spyOn(SuggestionClient.prototype, "search")
    .mockRejectedValueOnce(new Error("worker failed"))
    .mockResolvedValueOnce([{ value: "cat", indices: [] }]);
  const tags = ["cat"];
  const { result } = renderHook(() =>
    useSuggestions(tags, readActiveToken("ca", 2), new Set(), true, [])
  );
  await waitFor(() => expect(result.current.failed).toBe(true));
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.suggestions[0]?.value).toBe("cat"));
  expect(search).toHaveBeenCalledTimes(2);
});
