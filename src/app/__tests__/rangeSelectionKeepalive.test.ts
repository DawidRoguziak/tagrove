import { renderHook } from "@solidjs/testing-library";
import { flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAppRuntime } from "../createAppRuntime";
import { LanguageProvider } from "../../i18n/solid";
import { settle } from "../../test/settle";
import { idRangeBuffer } from "../../test/idRange";
import { summary } from "../../features/lightbox/__tests__/fixtures";
import { MATERIALIZE_LIMIT, SESSION_KEEPALIVE_MS } from "../../features/gallery/createSelection";
import { STALE_REFRESH_DELAY_MS } from "../../library/query/createAssetQuery";
import type { SelectionSpec } from "../../platform/tauri/contracts/assets";

const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false, invoke: api.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

/** The backend session rules that matter here (`asset_query_service.rs`): a session expires
 * after five minutes without a lookup, every lookup restarts that, and a changed dependency
 * revision makes it stale. */
const SESSION_TTL_MS = 5 * 60_000;
const TOTAL = MATERIALIZE_LIMIT * 2;
let revision: number;
let nextSessionId: number;
let failStart: boolean;
let sessions: Map<number, { revision: number; lastAccess: number }>;
const ready = {
  status: "ready",
  processed_assets: TOTAL,
  updated_assets: TOTAL,
  revision: 9,
  query_impact: { type: "none" },
  vocabulary_changed: false,
  is_favorite: null,
  media_group_key: null
};

function lookup(id: number) {
  for (const [key, session] of sessions)
    if (Date.now() - session.lastAccess > SESSION_TTL_MS) sessions.delete(key);
  const session = sessions.get(id);
  if (!session) return null;
  session.lastAccess = Date.now();
  if (session.revision === revision) return session;
  sessions.delete(id);
  return null;
}

beforeEach(() => {
  vi.useFakeTimers();
  revision = 1;
  nextSessionId = 1;
  failStart = false;
  sessions = new Map();
  api.invoke.mockReset().mockImplementation(async (command: string, args: Record<string, unknown> = {}) => {
    switch (command) {
      case "start_asset_query": {
        if (failStart) throw new Error("database is locked");
        let id = [...sessions].find(([key, session]) => session.revision === revision && lookup(key))?.[0];
        if (id === undefined) {
          id = nextSessionId++;
          sessions.set(id, { revision, lastAccess: Date.now() });
        }
        return { status: "ready", session_id: id, revision, total: TOTAL, offset: 0, items: [summary(1)] };
      }
      case "get_asset_query_sections": {
        const id = args.sessionId as number;
        return lookup(id) ? { status: "ready", session_id: id, total: TOTAL, sections: [] } : { status: "stale" };
      }
      case "get_asset_query_id_range": {
        if (!lookup(args.sessionId as number)) return new ArrayBuffer(0);
        const start = args.start as number;
        const end = Math.min(args.end as number, TOTAL - 1);
        return idRangeBuffer(Array.from({ length: end - start + 1 }, (_, i) => start + i + 1));
      }
      case "apply_selection_mutation":
        return lookup((args.selection as SelectionSpec).sessionId) ? ready : { status: "stale" };
      case "list_tags":
        return { items: [], total: 0 };
      default:
        throw new Error(`Unexpected command: ${command}`);
    }
  });
});
afterEach(() => {
  vi.useRealTimers();
});

const calls = (command: string) => api.invoke.mock.calls.filter(([name]) => name === command);
/** Loads the gallery, lets its session sit unread for `idleBefore` ms, then selects all. */
async function selectAll(idleBefore = 0, held = true) {
  const { result, cleanup } = renderHook(createAppRuntime, { wrapper: LanguageProvider });
  await settle(() => result.library.refreshLibrary());
  await vi.advanceTimersByTimeAsync(idleBefore);
  settle(() => result.selection.toggle());
  await settle(() => result.selection.interact({ type: "select-all" }));
  if (held) {
    expect(result.bulk.selectionRanged).toBe(true);
    expect(result.bulk.selectedCount).toBe(TOTAL);
  }
  return { runtime: result, cleanup };
}

describe("a held range selection and the session TTL", () => {
  it("survives idle time beyond the TTL and the next bulk write applies to it", async () => {
    const { runtime, cleanup } = await selectAll();
    await vi.advanceTimersByTimeAsync(SESSION_TTL_MS + 60_000);
    flush();
    expect(calls("get_asset_query_id_range").length).toBeGreaterThanOrEqual(5);
    expect(runtime.bulk.selectionRanged).toBe(true);
    expect(runtime.bulk.selectionNotice).toBeNull();

    await expect(settle(() => runtime.bulk.onAddTag("sunset"))).resolves.toBe(true);
    expect(calls("apply_selection_mutation")).toHaveLength(1);
    expect(calls("apply_selection_mutation")[0][1]).toEqual({
      selection: { sessionId: 1, ranges: [[0, TOTAL - 1]], excluded: [], included: [] },
      mutation: { type: "mergeTags", tags: ["sunset"] }
    });
    expect(runtime.bulk.tagSaveFailed).toBe(false);
    expect(runtime.bulk.appliedBulkTags).toEqual(["sunset"]);
    expect(runtime.bulk.selectionRanged).toBe(true);
    cleanup();
  });

  it("restarts the TTL when the selection is made late in the session's idle window", async () => {
    const { runtime, cleanup } = await selectAll(SESSION_TTL_MS - 30_000);
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS);
    flush();
    expect(runtime.bulk.selectionRanged).toBe(true);
    await expect(settle(() => runtime.bulk.onAddTag("sunset"))).resolves.toBe(true);
    expect(runtime.bulk.tagSaveFailed).toBe(false);
    cleanup();
  });

  it("clears at once, with the expired notice, a selection made on a session that already expired", async () => {
    const { runtime, cleanup } = await selectAll(SESSION_TTL_MS + 60_000, false);
    // The read made with the selection is stale: the ranges go before any refresh.
    expect(runtime.library.query.sessionId()).toBe(1);
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectionNotice).toBe("rangeExpired");
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    flush();
    expect(runtime.library.query.sessionId()).toBe(2);
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectionNotice).toBe("rangeExpired");
    expect(calls("apply_selection_mutation")).toHaveLength(0);
    cleanup();
  });

  it("clears, with the expired notice, a selection whose query failed to refresh", async () => {
    const { runtime, cleanup } = await selectAll();
    failStart = true;
    await settle(async () => {
      await expect(runtime.library.query.refresh()).rejects.toThrow("database is locked");
    });
    expect(runtime.library.query.sessionId()).toBeNull();
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectedCount).toBe(0);
    expect(runtime.bulk.selectionNotice).toBe("rangeExpired");
    // No keepalive is left reading a session the query gave up, and no write can use it.
    const reads = calls("get_asset_query_id_range").length;
    await vi.advanceTimersByTimeAsync(SESSION_TTL_MS + 60_000);
    expect(calls("get_asset_query_id_range")).toHaveLength(reads);
    await expect(settle(() => runtime.bulk.onAddTag("late"))).resolves.toBe(false);
    expect(calls("apply_selection_mutation")).toHaveLength(0);
    cleanup();
  });

  it("clears with the expired notice when a stale write's refresh fails", async () => {
    const { runtime, cleanup } = await selectAll();
    revision = 2;
    failStart = true;
    await expect(settle(() => runtime.bulk.onAddTag("late"))).resolves.toBe(false);
    expect(runtime.bulk.appliedBulkTags).toEqual([]);
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    flush();
    expect(runtime.library.query.sessionId()).toBeNull();
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectionNotice).toBe("rangeExpired");
    expect(calls("apply_selection_mutation")).toHaveLength(1);
    cleanup();
  });

  it("stops reading once the selection is cleared, and the unread session then expires", async () => {
    const { runtime, cleanup } = await selectAll();
    const spec = runtime.selection.capture().spec;
    settle(() => runtime.selection.clear());
    const reads = calls("get_asset_query_id_range").length;
    await vi.advanceTimersByTimeAsync(SESSION_TTL_MS + 60_000);
    expect(calls("get_asset_query_id_range")).toHaveLength(reads);
    // Proves the TTL is real in this backend double: the same spec is now refused.
    expect(sessions.size).toBe(1);
    await expect(
      api.invoke("apply_selection_mutation", { selection: spec, mutation: { type: "toggleFavorite" } })
    ).resolves.toEqual({ status: "stale" });
    cleanup();
  });

  it("clears the selection with its notice when the library really changed", async () => {
    const { runtime, cleanup } = await selectAll();
    revision = 2; // for example a scan committed new assets
    await vi.advanceTimersByTimeAsync(SESSION_KEEPALIVE_MS + STALE_REFRESH_DELAY_MS);
    flush();
    expect(calls("start_asset_query")).toHaveLength(2);
    expect(runtime.library.query.sessionId()).toBe(2);
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectedCount).toBe(0);
    expect(runtime.bulk.selectionNotice).toBe("rangeCleared");
    expect(calls("apply_selection_mutation")).toHaveLength(0);
    cleanup();
  });

  it("refuses a write made before the keepalive noticed the change, then clears with the notice", async () => {
    const { runtime, cleanup } = await selectAll();
    revision = 2;
    await expect(settle(() => runtime.bulk.onAddTag("late"))).resolves.toBe(false);
    expect(runtime.bulk.appliedBulkTags).toEqual([]);
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    flush();
    // The stale write refreshed the query; its new session replaced the selection, so the
    // panel explains why instead of showing a save error for a selection that is gone.
    expect(runtime.library.query.sessionId()).toBe(2);
    expect(runtime.bulk.selectionRanged).toBe(false);
    expect(runtime.bulk.selectedCount).toBe(0);
    expect(runtime.bulk.selectionNotice).toBe("rangeCleared");
    cleanup();
  });
});
