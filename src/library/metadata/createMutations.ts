import { onCleanup } from "solid-js";
import * as assets from "../../platform/tauri/assets";
import * as tagApi from "../../platform/tauri/tags";
import type { AssetDetails, AssetSummary } from "../../platform/tauri/contracts/assets";
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
  function acquire(ids: number[]) {
    const tokens: AssetTagMutationToken[] = [];
    for (const id of new Set(ids)) {
      const token = tags.beginMutation(id);
      if (!token) {
        for (const held of tokens) tags.settleMutation(held);
        throw new Error("Metadata is busy. Retry after the current operation.");
      }
      tokens.push(token);
    }
    return tokens;
  }
  async function write<T>(
    ids: number[],
    request: () => Promise<T>,
    changes: (result: T) => Map<number, Patch>,
    after: (result: T, accepted: number[]) => void | Promise<unknown>
  ) {
    const cached = new Map(ids.map((id) => [id, details.get(id)]));
    const tokens = acquire(ids);
    try {
      const result = await request();
      const patches = changes(result);
      const accepted: number[] = [];
      const published = new Map<number, Patch>();
      for (const token of tokens) {
        const patch = patches.get(token.assetId);
        if (!tags.settleMutation(token, patch?.tags)) continue;
        accepted.push(token.assetId);
        if (!patch) continue;
        published.set(token.assetId, patch);
        const previous = cached.get(token.assetId);
        if (previous) details.put({ ...previous, ...patch });
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
      if (accepted.length) await after(result, accepted);
      return result;
    } finally {
      // Also releases active IPC accounting after reset, failure or owner disposal.
      for (const token of tokens) tags.settleMutation(token);
    }
  }
  return {
    subscribe(listener: (commit: MetadataCommit) => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    saveTags(id: number, next: string[]) {
      return write(
        [id],
        () => tagApi.setAssetTags(id, [...next]),
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
    setFavorite(id: number, favorite: boolean) {
      return write(
        [id],
        () => assets.setAssetFavorite(id, favorite),
        () => new Map([[id, { is_favorite: favorite }]]),
        () => {
          if (filters().appliedFavoritesOnly && !favorite) return query.refresh();
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
