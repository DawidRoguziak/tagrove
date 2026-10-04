import { settle } from "../../../test/settle";
import { renderHook } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { createAssetTagState } from "../createAssetTagState";

describe("createAssetTagState", () => {
  it("blocks details while a mutation is pending and publishes its canonical result", () => {
    const { result } = renderHook(() => createAssetTagState());
    const initialRead = result.captureGeneration(7);
    settle(() => {
      expect(result.publishDetails(7, ["cat"], initialRead)).toBe(true);
    });
    const pendingRead = result.captureGeneration(7);
    let mutation!: NonNullable<ReturnType<typeof result.beginMutation>>;
    settle(() => {
      mutation = result.beginMutation(7)!;
    });

    expect(result.publishDetails(7, ["stale"], pendingRead)).toBe(false);
    expect(result.get(7)?.tags).toEqual(["cat"]);

    settle(() => {
      expect(result.settleMutation(mutation, [" Cat ", "dog", "cat"])).toBe(true);
    });
    expect(result.get(7)?.tags).toEqual(["cat", "dog"]);
  });

  it("grants only one mutation token per asset until the owner settles", () => {
    const { result } = renderHook(() => createAssetTagState());
    const first = result.beginMutation(5);

    expect(first).not.toBeNull();
    expect(result.beginMutation(5)).toBeNull();
    settle(() => {
      expect(result.settleMutation(first!)).toBe(true);
    });
    expect(result.beginMutation(5)).not.toBeNull();
  });

  it("invalidates reads started during a mutation even when the mutation fails", () => {
    const { result } = renderHook(() => createAssetTagState());
    let mutation!: NonNullable<ReturnType<typeof result.beginMutation>>;
    settle(() => {
      mutation = result.beginMutation(3)!;
    });
    const readDuringMutation = result.captureGeneration(3);
    settle(() => {
      result.settleMutation(mutation);
    });

    expect(result.publishDetails(3, ["possibly stale"], readDuringMutation)).toBe(false);
    expect(result.get(3)).toBeNull();
  });

  it("holds maintenance until active mutations settle and rejects new mutations immediately", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const active = result.beginMutation(1)!;
    const events: string[] = [];

    let maintenance!: Promise<void>;
    settle(() => {
      maintenance = result.runWithMutationBarrier(async () => {
        events.push("maintenance");
      });
    });

    expect(result.beginMutation(2)).toBeNull();
    expect(events).toEqual([]);

    await settle(async () => {
      expect(result.settleMutation(active, ["saved"])).toBe(true);
      await maintenance;
    });

    expect(events).toEqual(["maintenance"]);
    const afterMaintenance = result.beginMutation(2);
    expect(afterMaintenance).not.toBeNull();
    settle(() => {
      result.settleMutation(afterMaintenance!);
    });
  });

  it("keeps active IPC accounted for across reset before allowing maintenance", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const active = result.beginMutation(4)!;
    let maintenanceStarted = false;
    const maintenance = result.runWithMutationBarrier(async () => {
      maintenanceStarted = true;
    });

    settle(() => result.reset());
    await Promise.resolve();
    expect(maintenanceStarted).toBe(false);

    await settle(async () => {
      expect(result.settleMutation(active, ["stale result"])).toBe(false);
      await maintenance;
    });
    expect(maintenanceStarted).toBe(true);
  });

  it("serializes maintenance requests and releases the barrier after thrown errors", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const events: string[] = [];
    const first = result.runWithMutationBarrier(async () => {
      events.push("first");
      throw new Error("maintenance failed");
    });
    const second = result.runWithMutationBarrier(async () => {
      events.push("second");
    });

    expect(result.beginMutation(8)).toBeNull();
    await settle(async () => {
      await expect(first).rejects.toThrow("maintenance failed");
      await second;
    });
    expect(events).toEqual(["first", "second"]);
    expect(result.beginMutation(8)).not.toBeNull();
  });

  it("cancels waiting maintenance on coordinator unmount instead of leaving it locked", async () => {
    const { result, cleanup: unmount } = renderHook(() => createAssetTagState());
    expect(result.beginMutation(12)).not.toBeNull();
    const maintenance = result.runWithMutationBarrier(async () => {});
    const rejection = expect(maintenance).rejects.toThrow(
      "Tag mutation coordinator is unavailable"
    );

    unmount();

    await rejection;
  });

  it("global reset rejects old reads while allowing a reused ID to load fresh details", () => {
    const { result } = renderHook(() => createAssetTagState());
    const oldRead = result.captureGeneration(9);
    settle(() => result.reset());
    const newRead = result.captureGeneration(9);

    expect(result.publishDetails(9, ["old identity"], oldRead)).toBe(false);
    settle(() => {
      expect(result.publishDetails(9, ["new-identity"], newRead)).toBe(true);
    });
    expect(result.get(9)?.tags).toEqual(["new-identity"]);
    expect(result.remove(9, oldRead)).toBe(false);
    expect(result.get(9)?.tags).toEqual(["new-identity"]);
  });

  it("keeps a deletion tombstone that rejects pending details", () => {
    const { result } = renderHook(() => createAssetTagState());
    const pendingRead = result.captureGeneration(11);
    settle(() => result.remove(11));

    expect(result.publishDetails(11, ["deleted"], pendingRead)).toBe(false);
    const readAfterDeletion = result.captureGeneration(11);
    expect(result.publishDetails(11, ["also deleted"], readAfterDeletion)).toBe(false);
    expect(result.get(11)).toBeNull();
  });
  it("bounds inactive records, pins editors and mutations, and rejects tokens after eviction", () => {
    const { result } = renderHook(() => createAssetTagState());
    const release = result.pin(1);
    const pinned = result.captureGeneration(1);
    const stale = result.captureGeneration(2);
    let mutation!: NonNullable<ReturnType<typeof result.beginMutation>>;
    settle(() => {
      result.publishDetails(1, ["pinned"], pinned);
      result.publishDetails(2, ["old"], stale);
      mutation = result.beginMutation(3)!;
      for (let id = 4; id < 519; id++)
        result.publishDetails(id, ["cached"], result.captureGeneration(id));
    });
    expect(result.get(1)?.tags).toEqual(["pinned"]);
    expect(result.get(2)).toBeNull();
    expect(result.publishDetails(2, ["stale"], stale)).toBe(false);
    settle(() => {
      expect(result.settleMutation(mutation, ["saved"])).toBe(true);
      release();
    });
    const retained = Array.from({ length: 518 }, (_, index) => result.get(index + 1)).filter(
      Boolean
    );
    expect(retained.length).toBeLessThanOrEqual(512);
    expect(result.get(3)?.tags).toEqual(["saved"]);
  });
});
