import { renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createSelection,
  MATERIALIZE_LIMIT,
  mergeRanges,
  rangesContain,
  SESSION_KEEPALIVE_MS
} from "../createSelection";
function mount(
  load = vi.fn(
    async (from: number, to: number): Promise<ArrayLike<number>> =>
      Array.from({ length: to - from + 1 }, (_, i) => from + i + 1)
  ),
  total = 1_000_000,
  keepAlive?: (sessionId: number) => Promise<unknown>
) {
  // Asset ID = index + 1 in session 1, as the default loader returns.
  let session: number | null = 1;
  const view = renderHook(() => {
    const [query, setQuery] = createSignal(0),
      [identity, setIdentity] = createSignal(0);
    return {
      selection: createSelection({
        queryEpoch: query,
        identityEpoch: identity,
        getIdsRange: load,
        sessionId: () => session,
        total: () => total,
        keepAlive
      }),
      setQuery,
      setIdentity,
      /** A query result arrives: the session changes (or stays, for a cache hit). */
      nextQuery(next: number | null) {
        session = next;
        setQuery((value) => value + 1);
        flush();
      }
    };
  });
  flush();
  view.result.selection.toggle();
  flush();
  return { ...view, model: view.result.selection, load };
}
const click = (assetId: number, assetIndex: number, shift = false, ctrlLike = false) => ({
  type: "click" as const,
  assetId,
  assetIndex,
  shift,
  ctrlLike
});
describe("shared gallery selection", () => {
  it("toggles additive clicks synchronously and clears when disabled", () => {
    const { model } = mount();
    model.interact(click(1, 0));
    model.interact(click(2, 1));
    model.interact(click(1, 0));
    flush();
    expect([...model.ids]).toEqual([2]);
    model.toggle();
    flush();
    expect(model.ids.size).toBe(0);
  });
  it("resolves global shift ranges and rectangle gaps with one envelope read", async () => {
    const { model, load } = mount();
    model.interact(click(1, 0));
    await model.interact(click(2051, 2050, true));
    flush();
    expect(model.ids.size).toBe(2051);
    expect(load).toHaveBeenCalledExactlyOnceWith(0, 2050);
    load.mockClear();
    await model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [
        { startIndex: 0, endIndex: 1 },
        { startIndex: 4, endIndex: 5 },
        { startIndex: 256, endIndex: 257 }
      ]
    });
    flush();
    expect([...model.ids]).toEqual([1, 2, 5, 6, 257, 258]);
    expect(load).toHaveBeenCalledExactlyOnceWith(0, 257);
  });
  it("selects a 100k shift range without reading IDs and keeps it when a read fails", async () => {
    const { model, load } = mount();
    model.interact(click(1, 0));
    const started = performance.now();
    await model.interact(click(100_000, 99_999, true));
    const elapsed = performance.now() - started;
    flush();
    expect(model.count).toBe(100_000);
    expect(model.ranged).toBe(true);
    expect(model.ids.size).toBe(0);
    expect(load).not.toHaveBeenCalled();
    expect(model.capture().spec).toEqual({
      sessionId: 1,
      ranges: [[0, 99_999]],
      excluded: [],
      included: []
    });
    // Stage 09 budget: < 50 ms without IPC (jsdom here).
    expect(elapsed).toBeLessThan(50);
    load.mockRejectedValueOnce(new Error("Asset ID range was cancelled"));
    await model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [{ startIndex: 0, endIndex: 9 }]
    });
    flush();
    expect(model.count).toBe(100_000);
    expect(model.error).toMatch(/cancelled/);
  });
  it("preserves selection and offers explicit retry for incomplete reads", async () => {
    const { model, load } = mount();
    model.interact(click(99, 98));
    load.mockResolvedValueOnce([1]);
    await model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [{ startIndex: 0, endIndex: 2 }]
    });
    flush();
    expect([...model.ids]).toEqual([99]);
    expect(model.error).toMatch(/incomplete/);
    expect(model.busy).toBe(false);
    await model.retry();
    flush();
    expect([...model.ids]).toEqual([1, 2, 3]);
    expect(model.error).toBeNull();
  });
  it.each([
    "query",
    "clear",
    "disable",
    "dispose",
    "identity"
  ])("discards a late selection after %s", async (action) => {
    const { model, load, result, cleanup } = mount();
    model.interact(click(9, 8));
    flush();
    let finish: (ids: number[]) => void = () => {};
    load.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    );
    const pending = model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [{ startIndex: 0, endIndex: 2 }]
    });
    if (action === "query") result.setQuery(1);
    if (action === "identity") result.setIdentity(1);
    if (action === "clear") model.clear();
    if (action === "disable") model.toggle();
    if (action === "dispose") cleanup();
    flush();
    finish([1, 2, 3]);
    await pending;
    flush();
    expect([...model.ids]).not.toEqual([1, 2, 3]);
  });
  it("retains IDs across queries, drops the anchor and clears identity resets", async () => {
    const { model, result, load } = mount();
    model.interact(click(1, 100));
    flush();
    result.setQuery(1);
    flush();
    await model.interact(click(2, 200, true));
    flush();
    expect([...model.ids]).toEqual([1, 2]);
    expect(load).not.toHaveBeenCalled();
    result.setIdentity(1);
    flush();
    expect(model.ids.size).toBe(0);
  });
});

it("rejects a resolved range when its query changes before the Solid batch settles", async () => {
  const { model, load, result } = mount();
  model.interact(click(99, 98));
  flush();
  let finish: (ids: number[]) => void = () => {};
  load.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      })
  );
  const pending = model.interact({
    type: "rectangle-commit",
    additive: false,
    ranges: [{ startIndex: 0, endIndex: 2 }]
  });
  finish([1, 2, 3]);
  result.setQuery(1);
  await pending;
  flush();
  expect([...model.ids]).toEqual([99]);
});

it("merges sorted, overlapping and adjacent ranges and searches them", () => {
  expect(
    mergeRanges([
      [10, 20],
      [0, 3],
      [4, 5],
      [15, 30],
      [40, 39]
    ])
  ).toEqual([
    [0, 5],
    [10, 30]
  ]);
  const ranges = mergeRanges([
    [0, 5],
    [10, 30],
    [100, 100]
  ]);
  expect([0, 5, 10, 30, 100].every((index) => rangesContain(ranges, index))).toBe(true);
  expect([6, 9, 31, 99, 101, -1].some((index) => rangesContain(ranges, index))).toBe(false);
  expect(rangesContain([], 0)).toBe(false);
});

it("selects a million results as one range, without materializing IDs", async () => {
  const { model, load } = mount();
  const started = performance.now();
  await model.interact({ type: "select-all" });
  const elapsed = performance.now() - started;
  flush();
  expect(elapsed).toBeLessThan(50);
  expect(load).not.toHaveBeenCalled();
  expect(model.count).toBe(1_000_000);
  expect(model.ids.size).toBe(0);
  expect(model.isSelected(0, 1)).toBe(true);
  expect(model.isSelected(999_999, 1_000_000)).toBe(true);
  expect(model.isSelected(999_999)).toBe(true);
  // Ctrl-click inside the range excludes, outside it includes; counts follow.
  model.interact(click(501, 500, false, true));
  flush();
  expect(model.isSelected(500, 501)).toBe(false);
  expect(model.count).toBe(999_999);
  model.interact(click(501, 500, false, true));
  flush();
  expect(model.count).toBe(1_000_000);
  model.interact(click(501, 500, false, true));
  flush();
  expect(model.capture().spec).toEqual({
    sessionId: 1,
    ranges: [[0, 999_999]],
    excluded: [501],
    included: []
  });
  // 1M lookups stay cheap: binary search over ranges plus a small map.
  const lookups = performance.now();
  let selected = 0;
  for (let index = 0; index < 1_000_000; index++) if (model.isSelected(index, index + 1)) selected++;
  expect(selected).toBe(999_999);
  expect(performance.now() - lookups).toBeLessThan(1_000);
});

it("adds Ctrl-clicks outside ranges to included and re-covers them with a larger range", async () => {
  const { model, load } = mount();
  model.interact(click(1, 0));
  await model.interact(click(20_000, 19_999, true));
  flush();
  model.interact(click(30_001, 30_000, false, true));
  model.interact(click(101, 100, false, true));
  flush();
  expect(model.count).toBe(20_000);
  expect(model.capture().spec?.included).toEqual([30_001]);
  expect(model.capture().spec?.excluded).toEqual([101]);
  // A Ctrl+Shift range over both re-selects the excluded ID and covers the included one.
  model.interact(click(1, 0, false, true));
  model.interact(click(1, 0, false, true));
  await model.interact(click(40_000, 39_999, true, true));
  flush();
  expect(load).not.toHaveBeenCalled();
  expect(model.capture().spec).toEqual({
    sessionId: 1,
    ranges: [[0, 39_999]],
    excluded: [],
    included: []
  });
  expect(model.count).toBe(40_000);
});

it("resolves small ranges to IDs that survive a new query, and clears large ones with a notice", async () => {
  const { model, result, load } = mount();
  model.interact(click(1, 0));
  await model.interact(click(MATERIALIZE_LIMIT, MATERIALIZE_LIMIT - 1, true));
  flush();
  expect(load).toHaveBeenCalledExactlyOnceWith(0, MATERIALIZE_LIMIT - 1);
  expect(model.ranged).toBe(false);
  expect(model.ids.size).toBe(MATERIALIZE_LIMIT);
  result.nextQuery(2);
  expect(model.ids.size).toBe(MATERIALIZE_LIMIT);
  expect(model.notice).toBeNull();

  // Same filters, same session (a cache hit): the ranges stay valid.
  await model.interact({ type: "select-all" });
  flush();
  result.nextQuery(2);
  expect(model.count).toBe(1_000_000);
  // A pending query (no session yet) changes nothing until its result arrives.
  result.nextQuery(null);
  expect(model.count).toBe(1_000_000);
  result.nextQuery(3);
  expect(model.count).toBe(0);
  expect(model.notice).toBe("rangeCleared");
  model.interact(click(5, 4));
  flush();
  expect(model.notice).toBeNull();
});

it("keeps the identity of an ID selection when only the query session changes", () => {
  const { model, result } = mount();
  model.interact(click(7, 6));
  model.interact(click(9, 8));
  flush();
  const key = model.key;
  const ids = model.ids;
  result.nextQuery(2);
  expect(model.key).toBe(key);
  expect(model.ids).toBe(ids);
  expect(model.count).toBe(2);
  expect(model.notice).toBeNull();
});

it("finds IDs kept from an earlier query when a large range covers them", async () => {
  const { model, result, load } = mount();
  model.interact(click(7, 6));
  flush();
  // In the next session asset 7 sits at index 20_005, which the new range covers.
  result.nextQuery(2);
  load.mockImplementationOnce(async (from: number, to: number) =>
    Array.from({ length: to - from + 1 }, (_, i) => (from + i === 20_005 ? 7 : 100_000 + from + i))
  );
  model.interact(click(100_000, 0));
  model.interact(click(100_000, 0));
  await model.interact(click(100_000 + 30_000, 30_000, true, true));
  flush();
  expect(load).toHaveBeenCalledExactlyOnceWith(0, 30_000);
  expect(model.count).toBe(30_001);
  expect(model.capture().spec).toEqual({
    sessionId: 2,
    ranges: [[0, 30_000]],
    excluded: [],
    included: []
  });
});

it("drops a settled failed retry on identity reset", async () => {
  const { model, result, load } = mount();
  load.mockRejectedValueOnce(new Error("failed read"));
  await model.interact({
    type: "rectangle-commit",
    ranges: [{ startIndex: 0, endIndex: 1 }],
    additive: false
  });
  flush();
  expect(model.error).toContain("failed read");
  result.setIdentity(1);
  flush();
  await model.retry();
  expect(load).toHaveBeenCalledOnce();
  expect(model.error).toBeNull();
  expect(model.ids.size).toBe(0);
});

describe("session keepalive of a range selection", () => {
  afterEach(() => {
    vi.useRealTimers();
  });
  const setup = () => {
    vi.useFakeTimers();
    const keepAlive = vi.fn(async (_sessionId: number) => {});
    return { ...mount(undefined, 1_000_000, keepAlive), keepAlive };
  };

  it("reads the session at once and then once per interval, only while ranges are held", async () => {
    const { model, keepAlive } = setup();
    // IDs held directly (clicks, small ranges) need no session.
    model.interact(click(1, 0));
    await model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [{ startIndex: 0, endIndex: 9 }]
    });
    flush();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 3);
    expect(keepAlive).not.toHaveBeenCalled();

    await model.interact({ type: "select-all" });
    flush();
    expect(model.ranged).toBe(true);
    // The session may already have been idle for most of its TTL: read it right away.
    expect(keepAlive).toHaveBeenCalledExactlyOnceWith(1);
    // Idle for longer than the backend's five-minute session TTL.
    await vi.advanceTimersByTimeAsync(6 * 60_000);
    expect(keepAlive).toHaveBeenCalledTimes(7);
    expect(new Set(keepAlive.mock.calls.map(([id]) => id))).toEqual(new Set([1]));
    // Edits inside the ranges keep the same timer and read nothing extra.
    model.interact(click(5, 4));
    await model.interact(click(200_000, 199_999, true, true));
    flush();
    expect(keepAlive).toHaveBeenCalledTimes(7);
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS);
    expect(keepAlive).toHaveBeenCalledTimes(8);
    expect(model.capture().spec?.sessionId).toBe(1);

    model.clear();
    flush();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 5);
    expect(keepAlive).toHaveBeenCalledTimes(8);
  });

  it("stops when the session changes, selection mode ends or the owner unmounts", async () => {
    const { model, result, keepAlive, cleanup } = setup();
    await model.interact({ type: "select-all" });
    flush();
    expect(keepAlive).toHaveBeenCalledExactlyOnceWith(1);
    result.nextQuery(2);
    expect(model.ranged).toBe(false);
    expect(model.notice).toBe("rangeCleared");
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 3);
    expect(keepAlive).toHaveBeenCalledOnce();

    await model.interact({ type: "select-all" });
    flush();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS);
    expect(keepAlive).toHaveBeenCalledTimes(3);
    expect(keepAlive).toHaveBeenLastCalledWith(2);
    model.toggle();
    flush();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 3);
    expect(keepAlive).toHaveBeenCalledTimes(3);

    model.toggle();
    await model.interact({ type: "select-all" });
    flush();
    expect(keepAlive).toHaveBeenCalledTimes(4);
    cleanup();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 3);
    expect(keepAlive).toHaveBeenCalledTimes(4);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps the timer when a refresh returns the same cached session", async () => {
    const { model, result, keepAlive } = setup();
    await model.interact({ type: "select-all" });
    flush();
    result.nextQuery(null);
    result.nextQuery(1);
    expect(model.ranged).toBe(true);
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS);
    expect(keepAlive).toHaveBeenCalledTimes(2);
    expect(keepAlive.mock.calls).toEqual([[1], [1]]);
  });

  it("never overlaps reads and retries after a failed one", async () => {
    vi.useFakeTimers();
    let release!: () => void;
    const keepAlive = vi
      .fn<(sessionId: number) => Promise<unknown>>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            release = resolve;
          })
      )
      .mockRejectedValueOnce(new Error("busy"))
      .mockResolvedValue(undefined);
    const { model } = mount(undefined, 1_000_000, keepAlive);
    await model.interact({ type: "select-all" });
    flush();
    // The immediate read is still pending through three ticks.
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 3);
    expect(keepAlive).toHaveBeenCalledTimes(1);
    release();
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS * 2);
    expect(keepAlive).toHaveBeenCalledTimes(3);
    expect(model.error).toBeNull();
    expect(model.ranged).toBe(true);
  });
});
