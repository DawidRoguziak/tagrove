import { renderHook } from "@solidjs/testing-library";
import { createSignal, flush } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { createSelection } from "../createSelection";
function mount(
  load = vi.fn(
    async (from: number, to: number): Promise<ArrayLike<number>> =>
      Array.from({ length: to - from + 1 }, (_, i) => from + i + 1)
  )
) {
  const view = renderHook(() => {
    const [query, setQuery] = createSignal(0),
      [identity, setIdentity] = createSignal(0);
    return {
      selection: createSelection({ queryEpoch: query, identityEpoch: identity, getIdsRange: load }),
      setQuery,
      setIdentity
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
  it("selects a 100k shift range with one read and keeps the old selection when it fails", async () => {
    const ids = Float64Array.from({ length: 100_000 }, (_, i) => i + 1);
    const load = vi.fn(
      async (from: number, to: number): Promise<ArrayLike<number>> => ids.subarray(from, to + 1)
    );
    const { model } = mount(load);
    model.interact(click(1, 0));
    const started = performance.now();
    await model.interact(click(100_000, 99_999, true));
    const elapsed = performance.now() - started;
    flush();
    expect(model.ids.size).toBe(100_000);
    expect(load).toHaveBeenCalledExactlyOnceWith(0, 99_999);
    // Frontend share of the < 200 ms budget (jsdom, no IPC); recorded in the stage notes.
    expect(elapsed).toBeLessThan(200);
    load.mockRejectedValueOnce(new Error("Asset ID range was cancelled"));
    await model.interact({
      type: "rectangle-commit",
      additive: false,
      ranges: [{ startIndex: 0, endIndex: 9 }]
    });
    flush();
    expect(model.ids.size).toBe(100_000);
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
