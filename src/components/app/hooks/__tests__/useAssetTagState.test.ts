import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAssetTagState } from "../useAssetTagState";

describe("useAssetTagState", () => {
  it("blocks details while a mutation is pending and publishes its canonical result", () => {
    const { result } = renderHook(() => useAssetTagState());
    const initialRead = result.current.captureGeneration(7);
    act(() => {
      expect(result.current.publishDetails(7, ["cat"], initialRead)).toBe(true);
    });
    const pendingRead = result.current.captureGeneration(7);
    let mutation!: NonNullable<ReturnType<typeof result.current.beginMutation>>;
    act(() => {
      mutation = result.current.beginMutation(7)!;
    });

    expect(result.current.publishDetails(7, ["stale"], pendingRead)).toBe(false);
    expect(result.current.get(7)?.tags).toEqual(["cat"]);

    act(() => {
      expect(result.current.settleMutation(mutation, [" Cat ", "dog", "cat"])).toBe(true);
    });
    expect(result.current.get(7)?.tags).toEqual(["cat", "dog"]);
  });

  it("grants only one mutation token per asset until the owner settles", () => {
    const { result } = renderHook(() => useAssetTagState());
    const first = result.current.beginMutation(5);

    expect(first).not.toBeNull();
    expect(result.current.beginMutation(5)).toBeNull();
    act(() => {
      expect(result.current.settleMutation(first!)).toBe(true);
    });
    expect(result.current.beginMutation(5)).not.toBeNull();
  });

  it("invalidates reads started during a mutation even when the mutation fails", () => {
    const { result } = renderHook(() => useAssetTagState());
    let mutation!: NonNullable<ReturnType<typeof result.current.beginMutation>>;
    act(() => {
      mutation = result.current.beginMutation(3)!;
    });
    const readDuringMutation = result.current.captureGeneration(3);
    act(() => { result.current.settleMutation(mutation); });

    expect(result.current.publishDetails(3, ["possibly stale"], readDuringMutation)).toBe(false);
    expect(result.current.get(3)).toBeNull();
  });

  it("holds maintenance until active mutations settle and rejects new mutations immediately", async () => {
    const { result } = renderHook(() => useAssetTagState());
    const active = result.current.beginMutation(1)!;
    const events: string[] = [];

    let maintenance!: Promise<void>;
    act(() => {
      maintenance = result.current.runWithMutationBarrier(async () => {
        events.push("maintenance");
      });
    });

    expect(result.current.beginMutation(2)).toBeNull();
    expect(events).toEqual([]);

    await act(async () => {
      expect(result.current.settleMutation(active, ["saved"])).toBe(true);
      await maintenance;
    });

    expect(events).toEqual(["maintenance"]);
    const afterMaintenance = result.current.beginMutation(2);
    expect(afterMaintenance).not.toBeNull();
    act(() => { result.current.settleMutation(afterMaintenance!); });
  });

  it("keeps active IPC accounted for across reset before allowing maintenance", async () => {
    const { result } = renderHook(() => useAssetTagState());
    const active = result.current.beginMutation(4)!;
    let maintenanceStarted = false;
    const maintenance = result.current.runWithMutationBarrier(async () => {
      maintenanceStarted = true;
    });

    act(() => result.current.reset());
    await Promise.resolve();
    expect(maintenanceStarted).toBe(false);

    await act(async () => {
      expect(result.current.settleMutation(active, ["stale result"])).toBe(false);
      await maintenance;
    });
    expect(maintenanceStarted).toBe(true);
  });

  it("serializes maintenance requests and releases the barrier after thrown errors", async () => {
    const { result } = renderHook(() => useAssetTagState());
    const events: string[] = [];
    const first = result.current.runWithMutationBarrier(async () => {
      events.push("first");
      throw new Error("maintenance failed");
    });
    const second = result.current.runWithMutationBarrier(async () => {
      events.push("second");
    });

    expect(result.current.beginMutation(8)).toBeNull();
    await act(async () => {
      await expect(first).rejects.toThrow("maintenance failed");
      await second;
    });
    expect(events).toEqual(["first", "second"]);
    expect(result.current.beginMutation(8)).not.toBeNull();
  });

  it("cancels waiting maintenance on coordinator unmount instead of leaving it locked", async () => {
    const { result, unmount } = renderHook(() => useAssetTagState());
    expect(result.current.beginMutation(12)).not.toBeNull();
    const maintenance = result.current.runWithMutationBarrier(async () => {});
    const rejection = expect(maintenance).rejects.toThrow("Tag mutation coordinator is unavailable");

    unmount();

    await rejection;
  });

  it("global reset rejects old reads while allowing a reused ID to load fresh details", () => {
    const { result } = renderHook(() => useAssetTagState());
    const oldRead = result.current.captureGeneration(9);
    act(() => result.current.reset());
    const newRead = result.current.captureGeneration(9);

    expect(result.current.publishDetails(9, ["old identity"], oldRead)).toBe(false);
    act(() => {
      expect(result.current.publishDetails(9, ["new identity"], newRead)).toBe(true);
    });
    expect(result.current.get(9)?.tags).toEqual(["new identity"]);
    expect(result.current.remove(9, oldRead)).toBe(false);
    expect(result.current.get(9)?.tags).toEqual(["new identity"]);
  });

  it("keeps a deletion tombstone that rejects pending details", () => {
    const { result } = renderHook(() => useAssetTagState());
    const pendingRead = result.current.captureGeneration(11);
    act(() => result.current.remove(11));

    expect(result.current.publishDetails(11, ["deleted"], pendingRead)).toBe(false);
    const readAfterDeletion = result.current.captureGeneration(11);
    expect(result.current.publishDetails(11, ["also deleted"], readAfterDeletion)).toBe(false);
    expect(result.current.get(11)).toBeNull();
  });
});
