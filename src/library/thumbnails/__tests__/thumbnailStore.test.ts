import { describe, expect, it, vi } from "vitest";
import { ThumbnailStore } from "../thumbnailStore";

describe("ThumbnailStore", () => {
  it("notifies only changed assets and releases versions after the last consumer leaves", () => {
    const store = new ThumbnailStore();
    const changed = vi.fn();
    const other = vi.fn();
    const unsubscribe = store.subscribe(1, changed);
    store.subscribe(2, other);
    store.markRendering([1], true);
    store.complete({ 1: "one.jpg" }, [1]);
    expect(changed).toHaveBeenCalledTimes(2);
    expect(other).not.toHaveBeenCalled();
    const readyVersion = store.getVersion(1);
    store.sync({});
    expect(store.getVersion(1)).toBeGreaterThan(readyVersion);
    unsubscribe();
    expect(store.getVersion(1)).toBe(0);
    store.complete({ 1: "new.jpg" }, [1]);
    expect(store.getVersion(1)).toBeGreaterThan(readyVersion);
    store.clear();
    expect(store.getVersion(1)).toBe(0);
  });
});
