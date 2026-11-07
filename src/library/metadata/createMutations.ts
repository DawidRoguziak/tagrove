import { onCleanup } from "solid-js";
import * as assets from "../../platform/tauri/assets";
import * as tagApi from "../../platform/tauri/tags";
import type {
  AssetDetails,
  AssetSummary,
  SelectionMutation,
  SelectionMutationResult,
  SelectionSpec
} from "../../platform/tauri/contracts/assets";
import type { TagQueryImpact } from "../../platform/tauri/contracts/tags";
import type { createAssetQuery, LibraryFilters } from "../query/createAssetQuery";
import type { createDetailsCache } from "./createDetailsCache";
import type { AssetTagStateController, AssetTagMutationToken } from "./createAssetTagState";
import type { createKnownTags } from "../vocabulary/createKnownTags";
import { tagMutationRequiresRefresh } from "./queryImpact";

type Patch = Partial<
  Pick<AssetDetails, "tags" | "is_favorite" | "media_group_key" | "media_group_order">
>;
export type MetadataCommit =
  | { type: "updated"; patches: ReadonlyMap<number, Patch> }
  | { type: "deleted"; ids: readonly number[] };
interface Options {
  tags: AssetTagStateController;
  details: ReturnType<typeof createDetailsCache>;
  query: ReturnType<typeof createAssetQuery>;
  vocabulary: ReturnType<typeof createKnownTags>;
  filters: () => LibraryFilters;
}

/** Every caller shares the same admission, backend-first publication and invalidation. */
export function createMutations({ tags, details, query, vocabulary, filters }: Options) {
  const listeners = new Set<(commit: MetadataCommit) => void>();
  onCleanup(() => listeners.clear());
  function refresh() {
    void query.refresh().catch(() => {});
  }
  function tagRefresh(impact: TagQueryImpact, vocabularyChanged: boolean, missing: boolean) {
    const current = filters();
    if (
      missing ||
      tagMutationRequiresRefresh(impact, {
        include: current.filterInclude,
        exclude: current.filterExclude,
        metaFilter: current.metaFilter ?? null
      })
    )
      refresh();
    void vocabulary.refreshKnownTagsAfterMutation(vocabularyChanged).catch(() => {});
  }
  /**
   * Queues the write behind earlier writes on any of its IDs. Only maintenance, deletion,
   * an identity reset or disposal reject it; the base state is read once the turn arrives.
   */
  async function write<T>(
    ids: number[],
    request: () => Promise<T>,
    changes: (result: T) => Map<number, Patch>,
    after: (result: T, accepted: number[]) => void | Promise<unknown>
  ) {
    const unique = [...new Set(ids)];
    const admission = tags.enqueueMutation(unique);
    // An idle asset starts in the same tick; only a busy one waits for its predecessors.
    const turn = admission instanceof Promise ? await admission : admission;
    let tokens: AssetTagMutationToken[] = [];
    let result: T;
    const accepted: number[] = [];
    try {
      // Captured after earlier queued writes settled; starting tokens invalidates the cache.
      const cached = new Map(unique.map((id) => [id, details.get(id)]));
      // Canonical metadata may survive a refresh; captured path/size must not.
      const detailsGeneration = details.generation;
      tokens = turn.begin();
      result = await request();
      const patches = changes(result);
      const published = new Map<number, Patch>();
      for (const token of tokens) {
        const patch = patches.get(token.assetId);
        if (!tags.settleMutation(token, patch?.tags)) continue;
        accepted.push(token.assetId);
        if (!patch) continue;
        published.set(token.assetId, patch);
        const previous = cached.get(token.assetId);
        if (previous) details.put({ ...previous, ...patch }, detailsGeneration);
      }
      if (published.size) {
        query.patchAssets((rows) =>
          rows.map((row) => {
            const patch = published.get(row.id);
            if (!patch) return row;
            // Summary caches never carry complete tags or detail-only fields.
            const { tags: _tags, ...summaryPatch } = patch;
            return { ...row, ...summaryPatch } satisfies AssetSummary;
          })
        );
        for (const listener of listeners) listener({ type: "updated", patches: published });
      }
    } finally {
      // Settling also advances generations after failure, reset or owner disposal.
      for (const token of tokens) tags.settleMutation(token);
      // Secondary refreshes below must not hold the next queued write. `after` still starts
      // synchronously, so its local patches (such as a deletion tombstone) precede that write.
      turn.release();
    }
    if (accepted.length) await after(result, accepted);
    return result;
  }
  type SelectionReady = Extract<SelectionMutationResult, { status: "ready" }>;
  /**
   * A write on a selection spec, resolved by the backend (stage 09). It takes the
   * library-wide queue turn: every earlier write finishes first and later writes wait.
   * The affected IDs are unknown here, so every cached tag list and detail is dropped and
   * visible rows are re-read. `stale` wrote nothing; the query is refreshed so the
   * selection learns that its session is gone.
   */
  async function writeSelection(
    selection: SelectionSpec,
    mutation: SelectionMutation,
    after: (result: SelectionReady) => void
  ): Promise<SelectionMutationResult> {
    const admission = tags.enqueueLibraryMutation();
    const turn = admission instanceof Promise ? await admission : admission;
    let result: SelectionMutationResult;
    try {
      turn.begin();
      details.clear();
      result = await assets.applySelectionMutation(selection, mutation);
    } finally {
      // Detail reads that ran during the write saw old metadata.
      details.clear();
      turn.release();
    }
    if (result.status === "stale") refresh();
    else after(result);
    return result;
  }
  function canonicalTags(id: number) {
    const known = tags.get(id);
    if (!known) throw new Error("The complete tag list is not loaded.");
    return known.tags;
  }
  return {
    subscribe(listener: (commit: MetadataCommit) => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /**
     * Replaces the complete tag list. A function receives the canonical tags when the
     * write reaches the head of the asset's queue, so it cannot undo an earlier write.
     */
    saveTags(id: number, next: readonly string[] | ((canonical: string[]) => readonly string[])) {
      return write(
        [id],
        () =>
          tagApi.setAssetTags(id, [
            ...(typeof next === "function" ? next(canonicalTags(id)) : next)
          ]),
        (result) => new Map([[result.asset_id, { tags: result.tags }]]),
        (result) => tagRefresh(result.query_impact, result.vocabulary_changed, false)
      );
    },
    mergeTags(ids: number[], next: string[]) {
      const captured = [...new Set(ids)];
      return write(
        captured,
        () => tagApi.mergeAssetTagsBulk(captured, [...next]),
        (result) => new Map(result.results.map((item) => [item.asset_id, { tags: item.tags }])),
        (result) =>
          tagRefresh(
            result.query_impact,
            result.vocabulary_changed,
            result.processed_assets < captured.length
          )
      );
    },
    /** Adds tags to every asset of a selection spec. */
    mergeTagsInSelection(selection: SelectionSpec, next: string[]) {
      return writeSelection(selection, { type: "mergeTags", tags: [...next] }, (result) =>
        tagRefresh(result.query_impact, result.vocabulary_changed, false)
      );
    },
    /** Favorites a selection spec unless all of it already is, then clears it. */
    toggleFavoritesInSelection(selection: SelectionSpec) {
      // Summaries carry `is_favorite`, so visible rows are re-read.
      return writeSelection(selection, { type: "toggleFavorite" }, refresh);
    },
    /** Groups a selection spec in selection order, or clears its groups with `null`. */
    setMediaGroupInSelection(selection: SelectionSpec, key: string | null) {
      return writeSelection(
        selection,
        { type: "setMediaGroup", mediaGroupKey: key?.trim() || null },
        refresh
      );
    },
    setFavorite(id: number, favorite: boolean) {
      return write(
        [id],
        () => assets.setAssetFavorite(id, favorite),
        () => new Map([[id, { is_favorite: favorite }]]),
        () => {
          if (filters().appliedFavoritesOnly && !favorite) refresh();
        }
      );
    },
    toggleFavorites(ids: number[]) {
      const captured = [...new Set(ids)];
      return write(
        captured,
        () => assets.toggleAssetsFavoriteBulk(captured),
        (result) =>
          new Map(
            result.processed_asset_ids.map((id) => [id, { is_favorite: result.is_favorite }])
          ),
        (result) => {
          if (filters().appliedFavoritesOnly || result.processed_asset_ids.length < captured.length)
            refresh();
        }
      );
    },
    setMediaGroup(id: number, key: string | null, order: number | null) {
      const normalized = key?.trim() || null;
      return write(
        [id],
        () => assets.setAssetMediaGroup(id, normalized, order),
        () => new Map([[id, { media_group_key: normalized, media_group_order: order }]]),
        refresh
      );
    },
    setMediaGroups(
      updates: Array<{ assetId: number; mediaGroupOrder: number | null }>,
      key: string | null
    ) {
      const captured = updates.map((update) => ({ ...update }));
      const orders = new Map(captured.map((update) => [update.assetId, update.mediaGroupOrder]));
      return write(
        captured.map((update) => update.assetId),
        () => assets.setAssetsMediaGroupBulk(captured, key),
        (result) =>
          new Map(
            result.processed_asset_ids.map((id) => [
              id,
              { media_group_key: result.media_group_key, media_group_order: orders.get(id) ?? null }
            ])
          ),
        refresh
      );
    },
    deleteAsset(id: number) {
      return write(
        [id],
        () => assets.deleteAsset(id),
        () => new Map(),
        () => {
          tags.remove(id);
          details.remove(id);
          query.patchAssets((rows) => rows.filter((row) => row.id !== id));
          for (const listener of listeners) listener({ type: "deleted", ids: [id] });
          return Promise.allSettled([query.refresh(), vocabulary.refreshKnownTags()]);
        }
      );
    }
  };
}
