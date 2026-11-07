import { settle } from "../../../test/settle";
import { renderHook } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { createAssetTagState, type AssetTagStateController } from "../createAssetTagState";

/** Admits a write and starts its tokens once it reaches the head of every queue. */
async function begin(state: AssetTagStateController, ids: number[]) {
  const turn = await state.enqueueMutation(ids);
  return { turn, tokens: turn.begin() };
}
const flush = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};

describe("createAssetTagState", () => {
  it("blocks details while a mutation is pending and publishes its canonical result", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const initialRead = result.captureGeneration(7);
    settle(() => {
      expect(result.publishDetails(7, ["cat"], initialRead)).toBe(true);
    });
    const pendingRead = result.captureGeneration(7);
    const {
      turn,
      tokens: [mutation]
    } = await begin(result, [7]);

    expect(result.publishDetails(7, ["stale"], pendingRead)).toBe(false);
    expect(result.get(7)?.tags).toEqual(["cat"]);

    settle(() => {
      expect(result.settleMutation(mutation, [" Cat ", "dog", "cat"])).toBe(true);
    });
    turn.release();
    expect(result.get(7)?.tags).toEqual(["cat", "dog"]);
  });

  it("queues writes on one asset in admission order and lets a failed write release the next", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const order: string[] = [];
    const first = await begin(result, [5]);
    const second = Promise.resolve(result.enqueueMutation([5])).then((turn) => {
      order.push("second");
      return turn;
    });
    const third = Promise.resolve(result.enqueueMutation([5])).then((turn) => {
      order.push("third");
      return turn;
    });
    await flush();
    expect(order).toEqual([]);

    // The first write fails: it settles without tags and releases its slot.
    settle(() => result.settleMutation(first.tokens[0]));
    first.turn.release();
    const secondTurn = await second;
    expect(order).toEqual(["second"]);
    const [secondToken] = secondTurn.begin();
    secondTurn.release();
    await flush();
    expect(order).toEqual(["second", "third"]);
    expect(result.settleMutation(secondToken)).toBe(true);
    (await third).release();
  });

  it("orders overlapping multi-asset writes by admission without deadlock", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const started: string[] = [];
    const a = await begin(result, [2, 1]);
    const b = Promise.resolve(result.enqueueMutation([3, 2])).then((turn) => {
      started.push("b");
      return turn;
    });
    const c = Promise.resolve(result.enqueueMutation([1, 3])).then((turn) => {
      started.push("c");
      return turn;
    });
    const d = Promise.resolve(result.enqueueMutation([4])).then((turn) => {
      started.push("d");
      return turn;
    });
    await flush();
    expect(started).toEqual(["d"]);
    a.turn.release();
    const bTurn = await b;
    await flush();
    expect(started).toEqual(["d", "b"]);
    bTurn.release();
    (await c).release();
    (await d).release();
    expect(started).toEqual(["d", "b", "c"]);
  });

  it("invalidates reads started during a mutation even when the mutation fails", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const {
      turn,
      tokens: [mutation]
    } = await begin(result, [3]);
    const readDuringMutation = result.captureGeneration(3);
    settle(() => {
      result.settleMutation(mutation);
    });
    turn.release();

    expect(result.publishDetails(3, ["possibly stale"], readDuringMutation)).toBe(false);
    expect(result.get(3)).toBeNull();
  });

  it("holds maintenance until queued and active writes finish and rejects new admissions immediately", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const active = await begin(result, [1]);
    const queued = result.enqueueMutation([1]);
    const events: string[] = [];

    let maintenance!: Promise<void>;
    settle(() => {
      maintenance = result.runWithMutationBarrier(async () => {
        events.push("maintenance");
      });
    });

    expect(() => result.enqueueMutation([2])).toThrow("paused during maintenance");
    settle(() => {
      expect(result.settleMutation(active.tokens[0], ["saved"])).toBe(true);
    });
    active.turn.release();
    await flush();
    expect(events).toEqual([]);

    // A write admitted before the barrier still runs before maintenance.
    const queuedTurn = await queued;
    const [queuedToken] = queuedTurn.begin();
    settle(() => result.settleMutation(queuedToken));
    await settle(async () => {
      queuedTurn.release();
      await maintenance;
    });

    expect(events).toEqual(["maintenance"]);
    const afterMaintenance = await begin(result, [2]);
    expect(afterMaintenance.tokens).toHaveLength(1);
    afterMaintenance.turn.release();
  });

  it("keeps admitted writes accounted for across reset and rejects their late start", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const active = await begin(result, [4]);
    const queued = result.enqueueMutation([4]);
    let maintenanceStarted = false;
    const maintenance = result.runWithMutationBarrier(async () => {
      maintenanceStarted = true;
    });

    settle(() => result.reset());
    await Promise.resolve();
    expect(maintenanceStarted).toBe(false);

    expect(result.settleMutation(active.tokens[0], ["stale result"])).toBe(false);
    active.turn.release();
    const queuedTurn = await queued;
    expect(() => queuedTurn.begin()).toThrow("library changed");
    await settle(async () => {
      queuedTurn.release();
      await maintenance;
    });
    expect(maintenanceStarted).toBe(true);
  });

  it("rejects a queued write for an asset deleted while it waited", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const active = await begin(result, [6]);
    const queued = result.enqueueMutation([6]);
    settle(() => {
      result.settleMutation(active.tokens[0]);
      result.remove(6);
    });
    active.turn.release();
    const queuedTurn = await queued;
    expect(() => queuedTurn.begin()).toThrow("deleted");
    queuedTurn.release();
    await result.waitForIdle();
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

    expect(() => result.enqueueMutation([8])).toThrow("paused during maintenance");
    await settle(async () => {
      await expect(first).rejects.toThrow("maintenance failed");
      await second;
    });
    expect(events).toEqual(["first", "second"]);
    (await begin(result, [8])).turn.release();
  });

  it("cancels waiting maintenance and queued writes on coordinator unmount", async () => {
    const { result, cleanup: unmount } = renderHook(() => createAssetTagState());
    const active = await begin(result, [12]);
    const queued = result.enqueueMutation([12]);
    const maintenance = result.runWithMutationBarrier(async () => {});
    const rejection = expect(maintenance).rejects.toThrow(
      "Tag mutation coordinator is unavailable"
    );

    unmount();

    await rejection;
    active.turn.release();
    const queuedTurn = await queued;
    expect(() => queuedTurn.begin()).toThrow("unavailable");
    expect(() => result.enqueueMutation([13])).toThrow("unavailable");
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

  it("bounds inactive records, pins editors and mutations, and rejects tokens after eviction", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const release = result.pin(1);
    const pinned = result.captureGeneration(1);
    const stale = result.captureGeneration(2);
    const {
      turn,
      tokens: [mutation]
    } = await begin(result, [3]);
    settle(() => {
      result.publishDetails(1, ["pinned"], pinned);
      result.publishDetails(2, ["old"], stale);
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
    turn.release();
    const retained = Array.from({ length: 518 }, (_, index) => result.get(index + 1)).filter(
      Boolean
    );
    expect(retained.length).toBeLessThanOrEqual(512);
    expect(result.get(3)?.tags).toEqual(["saved"]);
  });

  it("keeps every target of a large bulk write active while the inactive LRU prunes", async () => {
    const { result } = renderHook(() => createAssetTagState());
    const ids = Array.from({ length: 1200 }, (_, index) => index + 1);
    const { turn, tokens } = await begin(result, ids);
    settle(() => {
      for (const token of tokens) expect(result.settleMutation(token, ["bulk"])).toBe(true);
    });
    turn.release();
    expect(result.get(1200)?.tags).toEqual(["bulk"]);
  });
});

it("orders a selection-wide write after earlier writes and before later ones, then drops cached tags", async () => {
  const { result } = renderHook(() => createAssetTagState());
  const read = result.captureGeneration(4);
  settle(() => {
    expect(result.publishDetails(4, ["cat"], read)).toBe(true);
  });
  const order: string[] = [];
  const first = await begin(result, [4]);
  const library = Promise.resolve(result.enqueueLibraryMutation()).then((turn) => {
    order.push("library");
    return turn;
  });
  // A later single-asset write on an unrelated asset still waits for the library turn.
  const later = Promise.resolve(result.enqueueMutation([9])).then((turn) => {
    order.push("later");
    return turn;
  });
  await flush();
  expect(order).toEqual([]);
  settle(() => {
    result.settleMutation(first.tokens[0], ["cat", "dog"]);
  });
  first.turn.release();
  const turn = await library;
  await flush();
  expect(order).toEqual(["library"]);
  // A read started before the write cannot publish; one started during it is dropped
  // when the write ends.
  const before = result.captureGeneration(4);
  settle(() => turn.begin());
  expect(result.get(4)).toBeNull();
  const during = result.captureGeneration(4);
  settle(() => {
    expect(result.publishDetails(4, ["stale"], before)).toBe(false);
    expect(result.publishDetails(4, ["stale"], during)).toBe(true);
  });
  settle(() => turn.release());
  expect(result.get(4)).toBeNull();
  const next = await later;
  expect(order).toEqual(["library", "later"]);
  expect(next.begin()).toHaveLength(1);
  next.release();
  // With everything settled, new writes start in the same tick again.
  expect(result.enqueueMutation([9])).not.toBeInstanceOf(Promise);
});

it("rejects selection-wide writes during maintenance and after an identity reset", async () => {
  const { result } = renderHook(() => createAssetTagState());
  let rejected: unknown;
  await result.runWithMutationBarrier(async () => {
    try {
      result.enqueueLibraryMutation();
    } catch (error) {
      rejected = error;
    }
  });
  expect(String(rejected)).toMatch(/paused during maintenance/);
  const blocker = await begin(result, [1]);
  const queued = Promise.resolve(result.enqueueLibraryMutation());
  settle(() => result.reset());
  blocker.turn.release();
  const turn = await queued;
  expect(() => turn.begin()).toThrow(/library changed/);
  turn.release();
  await result.waitForIdle();
});
