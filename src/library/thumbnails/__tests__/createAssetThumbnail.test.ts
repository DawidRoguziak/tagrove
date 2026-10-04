import { renderHook } from "@solidjs/testing-library";
import { createEffect, createSignal } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { ThumbnailStore } from "../../../hooks/services/thumbnailStore";
import { settle } from "../../../test/settle";
import { createAssetThumbnail } from "../createAssetThumbnail";

describe("Solid asset thumbnail subscriptions", () => {
  it("updates only the affected tile, publishes path and rendering atomically, follows slot identity and disposes", () => {
    const store = new ThumbnailStore();
    const first = vi.fn();
    const second = vi.fn();
    const { result, cleanup } = renderHook(() => {
      const [id, setId] = createSignal(1);
      const one = createAssetThumbnail(store, id);
      const two = createAssetThumbnail(store, () => 2);
      createEffect(one, first);
      createEffect(two, second);
      return { one, two, setId };
    });
    settle(() => {});
    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    settle(() => store.markRendering([1], true));
    expect(first).toHaveBeenCalledTimes(2);
    settle(() => store.complete({ 1: "one.jpg" }, [1]));
    expect(first).toHaveBeenCalledTimes(3);
    expect(result.one()).toMatchObject({ path: "one.jpg", rendering: false });
    expect(second).toHaveBeenCalledOnce();
    settle(() => result.setId(3));
    expect(result.one()).toMatchObject({ id: 3, path: undefined, rendering: false });
    const count = first.mock.calls.length;
    settle(() => store.complete({ 1: "old.jpg" }, []));
    expect(first).toHaveBeenCalledTimes(count);
    settle(() => store.complete({ 2: "two.jpg", 3: "three.jpg" }, []));
    store.retain([]);
    expect(store.getPath(1)).toBeUndefined();
    expect(store.getPath(2)).toBe("two.jpg");
    expect(store.getPath(3)).toBe("three.jpg");
    cleanup();
    expect(store.getPath(2)).toBeUndefined();
    expect(store.getPath(3)).toBeUndefined();
    const finalCount = first.mock.calls.length;
    settle(() => store.markRendering([3], true));
    expect(first).toHaveBeenCalledTimes(finalCount);
  });
});
