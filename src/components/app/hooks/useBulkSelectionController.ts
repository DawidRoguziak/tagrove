import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { getAssetDetails, setAssetTags, toggleAssetsFavoriteBulk } from "../../../api";
import type { AssetDetails, AssetSummary } from "../../../types";
import { applyBulkMediaGroupAction } from "../../bulk/grouping/services/applyBulkMediaGroupAction";
import {
  deriveBulkGroupSelectionState,
  normalizeGroupKey,
  reorderAssetIdsByDrop
} from "../../bulk/grouping/services/bulkGroupOrderService";
import { applyBulkTagsAction } from "../../bulk/tagging/services/applyBulkTagsAction";
import {
  mergeTagLists,
  normalizeBulkTag
} from "../../bulk/tagging/services/bulkTagMergeService";
import type { BulkSelectionInteraction } from "../../gallery/GalleryGrid";
import {
  bulkTagMutationRequiresRefresh
} from "../services/libraryInvalidationService";
import { useAssetTagState } from "./useAssetTagState";
import type { AssetTagMutationToken, AssetTagStateController } from "./useAssetTagState";

interface CachedDetail {
  details: AssetDetails;
  epoch: number;
}

function getCachedDetail(
  cache: Map<number, CachedDetail>,
  assetId: number,
  epoch: number
): AssetDetails | null {
  const entry = cache.get(assetId);
  if (!entry) return null;
  if (entry.epoch !== epoch) {
    cache.delete(assetId);
    return null;
  }
  cache.delete(assetId);
  cache.set(assetId, entry);
  return entry.details;
}

function putCachedDetail(
  cache: Map<number, CachedDetail>,
  limit: number,
  details: AssetDetails,
  epoch: number
): void {
  cache.delete(details.id);
  cache.set(details.id, { details, epoch });
  while (cache.size > limit) {
    const oldest = cache.keys().next();
    if (oldest.done) break;
    cache.delete(oldest.value);
  }
}

interface UseBulkSelectionControllerOptions {
  assets: AssetSummary[];
  queryEpoch?: number;
  getIdsRangeAsync?: (startIndex: number, endIndex: number) => Promise<number[]>;
  knownTags: string[];
  settingsViewOpen: boolean;
  queueThumbnailsByIds: (assetIds: number[]) => void;
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  assetTagState?: AssetTagStateController;
  appliedFilterTags?: string[];
  appliedFavoritesOnly?: boolean;
  onFavoritesChanged?: (assetIds: ReadonlySet<number>, isFavorite: boolean) => void;
}

function normalizedGroupIdentity(value: string | null): string | null {
  const normalized = normalizeGroupKey(value ?? "");
  return normalized ? normalized.toLowerCase() : null;
}

export function useBulkSelectionController({
  assets,
  queryEpoch = 0,
  getIdsRangeAsync,
  knownTags,
  settingsViewOpen,
  queueThumbnailsByIds,
  setAssets,
  refresh,
  refreshKnownTags,
  assetTagState: sharedAssetTagState,
  appliedFavoritesOnly = false,
  onFavoritesChanged,
  appliedFilterTags = []
}: UseBulkSelectionControllerOptions) {
  const localAssetTagState = useAssetTagState();
  const assetTagState = sharedAssetTagState ?? localAssetTagState;
  const [selectionModeEnabled, setSelectionModeEnabled] = useState(false);
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<number>>(new Set());
  const [favoriteApplying, setFavoriteApplying] = useState(false);
  const [favoriteFailedSelection, setFavoriteFailedSelection] = useState<Set<number> | null>(null);
  const favoriteOperationRef = useRef(false);
  const [groupKeyDraft, setGroupKeyDraft] = useState("");
  const [orderedAssetIds, setOrderedAssetIds] = useState<number[]>([]);
  const [hasConflictingGroups, setHasConflictingGroups] = useState(false);
  const [groupApplying, setGroupApplying] = useState(false);
  const [groupFailed, setGroupFailed] = useState(false);
  const [singleAssetTags, setSingleAssetTags] = useState<string[]>([]);
  const [appliedBulkTags, setAppliedBulkTags] = useState<string[]>([]);
  const [tagDetailsLoading, setTagDetailsLoading] = useState(false);
  const [tagDetailsFailed, setTagDetailsFailed] = useState(false);
  const [tagDetailsRetry, setTagDetailsRetry] = useState(0);
  const [tagApplying, setTagApplying] = useState(false);
  const [tagSaveFailed, setTagSaveFailed] = useState(false);
  const detailsCacheRef = useRef<Map<number, CachedDetail>>(new Map());
  const DETAILS_CACHE_LIMIT = 256;
  const detailsRequestRef = useRef(0);
  const groupOperationRef = useRef(false);
  const tagOperationRef = useRef(false);
  const tagOperationGenerationRef = useRef(0);
  const rangeRequestRef = useRef(0);
  const observedTagEpochRef = useRef(assetTagState.epoch);
  const observedQueryEpochRef = useRef(queryEpoch);
  // Anchor position in the global session snapshot, independent of the
  // sparse render cache (eviction must not move or drop it).
  const selectionAnchorIndexRef = useRef<number | null>(null);
  const skipDetailsForEpochRef = useRef<number | null>(null);

  useEffect(() => {
    if (observedTagEpochRef.current === assetTagState.epoch) return;
    observedTagEpochRef.current = assetTagState.epoch;
    skipDetailsForEpochRef.current = assetTagState.epoch;
    detailsRequestRef.current += 1;
    detailsCacheRef.current.clear();
    tagOperationGenerationRef.current += 1;
    rangeRequestRef.current += 1;
    tagOperationRef.current = false;
    groupOperationRef.current = false;
    setSelectedAssetIds(new Set());
    setSingleAssetTags([]);
    setAppliedBulkTags([]);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    setTagApplying(false);
    setTagSaveFailed(false);
    setGroupApplying(false);
    setGroupFailed(false);
  }, [assetTagState.epoch]);

  const selectedAssets = useMemo(() => {
    if (!selectedAssetIds.size) return [];
    return assets.filter((asset) => selectedAssetIds.has(asset.id));
  }, [assets, selectedAssetIds]);
  const selectionKey = selectedAssets.map((asset) => asset.id).join(",");
  const selectionKeyRef = useRef(selectionKey);
  selectionKeyRef.current = selectionKey;

  useEffect(() => {
    if (selectionModeEnabled) return;
    setSelectedAssetIds(new Set());
    selectionAnchorIndexRef.current = null;
  }, [selectionModeEnabled]);

  // Selection is stored as bare IDs and survives page eviction. Records that
  // leave the active filter stay selected; they are simply not rendered.
  // A new query session reorders global indexes, so only the range anchor
  // (an index, not an ID) must be invalidated.
  useEffect(() => {
    if (observedQueryEpochRef.current === queryEpoch) return;
    observedQueryEpochRef.current = queryEpoch;
    // A new session may reuse IDs for changed rows; cached details are stale.
    detailsCacheRef.current.clear();
    selectionAnchorIndexRef.current = null;
    rangeRequestRef.current += 1;
  }, [queryEpoch]);


  useEffect(() => {
    const derived = deriveBulkGroupSelectionState(selectedAssets);
    setGroupKeyDraft(derived.groupKey);
    setOrderedAssetIds(derived.orderedAssetIds);
    setHasConflictingGroups(derived.hasConflictingGroups);
    setGroupFailed(false);
    setTagSaveFailed(false);
    setTagDetailsFailed(false);
    setAppliedBulkTags([]);
  }, [selectionKey]);

  useEffect(() => {
    if (skipDetailsForEpochRef.current === assetTagState.epoch) {
      skipDetailsForEpochRef.current = null;
      return;
    }
    const requestId = detailsRequestRef.current + 1;
    detailsRequestRef.current = requestId;
    if (!selectionModeEnabled || settingsViewOpen || selectedAssets.length !== 1) {
      setSingleAssetTags([]);
      setTagDetailsLoading(false);
      setTagDetailsFailed(false);
      return;
    }

    const asset = selectedAssets[0]!;
    const authoritative = assetTagState.get(asset.id);
    if (authoritative) {
      setSingleAssetTags(authoritative.tags);
      setTagDetailsLoading(false);
      setTagDetailsFailed(false);
      return;
    }

    const cached = getCachedDetail(detailsCacheRef.current, asset.id, assetTagState.epoch);
    if (cached) {
      const generation = assetTagState.captureGeneration(asset.id);
      const accepted = assetTagState.publishDetails(asset.id, cached.tags, generation);
      const authoritative = assetTagState.get(asset.id);
      if (!accepted && !authoritative) {
        detailsCacheRef.current.delete(asset.id);
        setSingleAssetTags([]);
        setTagDetailsLoading(false);
        setTagDetailsFailed(true);
        return;
      }
      setSingleAssetTags(authoritative?.tags ?? cached.tags);
      setTagDetailsLoading(false);
      setTagDetailsFailed(false);
      return;
    }

    setSingleAssetTags([]);
    setTagDetailsLoading(true);
    setTagDetailsFailed(false);
    const generation = assetTagState.captureGeneration(asset.id);
    void getAssetDetails(asset.id)
      .then((details) => {
        if (detailsRequestRef.current !== requestId || selectionKeyRef.current !== String(asset.id)) {
          return;
        }
        if (!details) {
          setTagDetailsFailed(true);
          return;
        }
        const accepted = assetTagState.publishDetails(details.id, details.tags, generation);
        const authoritative = assetTagState.get(details.id);
        if (!accepted && !authoritative) {
          setTagDetailsFailed(true);
          return;
        }
        const tags = authoritative?.tags ?? details.tags;
        putCachedDetail(detailsCacheRef.current, DETAILS_CACHE_LIMIT, { ...details, tags }, assetTagState.epoch);
        setSingleAssetTags(tags);
      })
      .catch(() => {
        if (detailsRequestRef.current === requestId && selectionKeyRef.current === String(asset.id)) {
          setTagDetailsFailed(true);
        }
      })
      .finally(() => {
        if (detailsRequestRef.current === requestId && selectionKeyRef.current === String(asset.id)) {
          setTagDetailsLoading(false);
        }
      });
  }, [assetTagState, selectionKey, selectionModeEnabled, settingsViewOpen, tagDetailsRetry]);

  useEffect(() => {
    if (selectedAssets.length !== 1) return;
    const authoritative = assetTagState.get(selectedAssets[0]!.id);
    if (!authoritative) return;
    setSingleAssetTags(authoritative.tags);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
  }, [assetTagState.revision, selectionKey]);

  useEffect(() => {
    if (selectionModeEnabled && selectedAssets.length > 1) {
      queueThumbnailsByIds(selectedAssets.map((asset) => asset.id));
    }
  }, [queueThumbnailsByIds, selectedAssets, selectionModeEnabled]);

  const onBulkSelectionInteraction = useCallback(
    (interaction: BulkSelectionInteraction) => {
      if (!selectionModeEnabled) return;
      const { assetId, assetIndex, ctrlLike, shift, viaDrag } = interaction;
      const requestId = rangeRequestRef.current + 1;
      rangeRequestRef.current = requestId;

      if (viaDrag) {
        setSelectedAssetIds((previous) => {
          if (previous.has(assetId)) return previous;
          const next = new Set(previous);
          next.add(assetId);
          return next;
        });
        selectionAnchorIndexRef.current = selectionAnchorIndexRef.current ?? assetIndex;
        return;
      }

      if (shift) {
        // Both ends are global session indexes; the tile-provided index is the
        // virtualizer index, never the compact cache array position. Without a
        // stored global anchor the Shift press degrades to a plain selection
        // instead of guessing an index from the sparse render cache.
        const anchorIndex = selectionAnchorIndexRef.current;
        if (anchorIndex !== null) {
          const from = Math.min(anchorIndex, assetIndex);
          const to = Math.max(anchorIndex, assetIndex);
          const rangeResolver = getIdsRangeAsync;
          if (rangeResolver) {
            const requestEpoch = queryEpoch;
            void rangeResolver(from, to).then((rangeAssetIds) => {
              if (rangeRequestRef.current !== requestId || observedQueryEpochRef.current !== requestEpoch) return;
              setSelectedAssetIds((previous) => {
                const next = ctrlLike ? new Set(previous) : new Set<number>();
                for (const id of rangeAssetIds) next.add(id);
                return next;
              });
            }).catch(() => {
              // A failed range read leaves the current selection untouched;
              // the next Shift click retries the whole range.
            });
            return;
          }
        }
      }

      selectionAnchorIndexRef.current = assetIndex;
      if (ctrlLike) {
        setSelectedAssetIds((previous) => {
          const next = new Set(previous);
          if (next.has(assetId)) next.delete(assetId);
          else next.add(assetId);
          return next;
        });
        return;
      }
      setSelectedAssetIds(new Set([assetId]));
    },
    [getIdsRangeAsync, queryEpoch, selectionModeEnabled]
  );

  const onToggleSelectionMode = useCallback(() => {
    if (selectionModeEnabled) rangeRequestRef.current += 1;
    setSelectionModeEnabled((previous) => !previous);
  }, [selectionModeEnabled]);

  const onToggleFavorite = useCallback(async () => {
    if (selectedAssetIds.size === 0 || favoriteOperationRef.current) return;
    const capturedIds = selectedAssetIds;
    const tokens: AssetTagMutationToken[] = [];
    for (const assetId of capturedIds) {
      const token = assetTagState.beginMutation(assetId);
      if (!token) {
        for (const acquired of tokens) assetTagState.settleMutation(acquired);
        setFavoriteFailedSelection(capturedIds);
        return;
      }
      tokens.push(token);
    }
    favoriteOperationRef.current = true;
    setFavoriteApplying(true);
    setFavoriteFailedSelection(null);
    try {
      const result = await toggleAssetsFavoriteBulk([...capturedIds]);
      const processedIds = new Set(result.processed_asset_ids);
      const acceptedIds = new Set<number>();
      const missingIds = new Set<number>();
      for (const token of tokens) {
        if (!assetTagState.settleMutation(token)) continue;
        if (processedIds.has(token.assetId)) acceptedIds.add(token.assetId);
        else missingIds.add(token.assetId);
      }
      tokens.length = 0;
      if (acceptedIds.size > 0) {
        setAssets((previous) => previous.map((asset) => acceptedIds.has(asset.id)
          ? { ...asset, is_favorite: result.is_favorite } : asset));
        for (const assetId of acceptedIds) {
          const cached = getCachedDetail(detailsCacheRef.current, assetId, assetTagState.epoch);
          if (cached) putCachedDetail(detailsCacheRef.current, DETAILS_CACHE_LIMIT,
            { ...cached, is_favorite: result.is_favorite }, assetTagState.epoch);
        }
        onFavoritesChanged?.(acceptedIds, result.is_favorite);
      }
      if (missingIds.size > 0) {
        setSelectedAssetIds((previous) => new Set([...previous].filter((id) => !missingIds.has(id))));
      }
      if (missingIds.size > 0 || (acceptedIds.size > 0 && appliedFavoritesOnly)) {
        void refresh().catch(() => {});
      }
    } catch {
      // An identity reset makes this result irrelevant to the current library.
      if (tokens.some((token) => assetTagState.captureGeneration(token.assetId).epoch === token.epoch)) {
        setFavoriteFailedSelection(capturedIds);
      }
    } finally {
      for (const token of tokens) assetTagState.settleMutation(token);
      favoriteOperationRef.current = false;
      setFavoriteApplying(false);
    }
  }, [appliedFavoritesOnly, assetTagState, onFavoritesChanged, refresh, selectedAssetIds, setAssets]);

  const onApplyGroup = useCallback(async () => {
    if (!selectedAssets.length || groupOperationRef.current) return;
    const selectedIdSet = new Set(selectedAssets.map((asset) => asset.id));
    const capturedOrderedIds = orderedAssetIds.filter((id) => selectedIdSet.has(id));
    const capturedSelectionKey = selectionKey;
    const normalizedDraft = normalizeGroupKey(groupKeyDraft);
    const singleAsset = selectedAssets.length === 1 ? selectedAssets[0]! : null;
    const preservesExistingGroup = Boolean(
      singleAsset &&
        normalizedDraft &&
        normalizedGroupIdentity(singleAsset.media_group_key) === normalizedDraft.toLowerCase()
    );
    const mutationTokens: AssetTagMutationToken[] = [];
    for (const assetId of capturedOrderedIds) {
      const token = assetTagState.beginMutation(assetId);
      if (!token) {
        for (const acquired of mutationTokens) assetTagState.settleMutation(acquired);
        setGroupFailed(true);
        return;
      }
      mutationTokens.push(token);
    }

    groupOperationRef.current = true;
    setGroupApplying(true);
    setGroupFailed(false);
    try {
      await applyBulkMediaGroupAction({
        assetIdsInOrder: capturedOrderedIds,
        groupKey: normalizedDraft,
        preservedSingleOrder: preservesExistingGroup ? singleAsset?.media_group_order : null,
        setAssets
      });

      const normalizedKey = normalizedDraft || null;
      capturedOrderedIds.forEach((assetId, index) => {
        const cached = getCachedDetail(detailsCacheRef.current, assetId, assetTagState.epoch);
        if (!cached) return;
        const order = normalizedKey
          ? capturedOrderedIds.length === 1 && preservesExistingGroup
            ? singleAsset?.media_group_order ?? 1
            : index + 1
          : null;
        putCachedDetail(
          detailsCacheRef.current,
          DETAILS_CACHE_LIMIT,
          {
            ...cached,
            media_group_key: normalizedKey,
            media_group_order: order
          },
          assetTagState.epoch
        );
      });

      if (selectionKeyRef.current === capturedSelectionKey) {
        setGroupKeyDraft(normalizedDraft);
        setHasConflictingGroups(false);
      }
      // Group changes alter ordering/adjacency of the active view, so restart
      // the query session instead of trusting the local patch alone.
      void refresh().catch(() => {});
    } catch {
      if (selectionKeyRef.current === capturedSelectionKey) setGroupFailed(true);
    } finally {
      for (const token of mutationTokens) assetTagState.settleMutation(token);
      groupOperationRef.current = false;
      setGroupApplying(false);
    }
  }, [assetTagState, groupKeyDraft, orderedAssetIds, refresh, selectedAssets, selectionKey, setAssets]);

  const onAddTag = useCallback(
    async (rawTag: string): Promise<boolean> => {
      const tag = normalizeBulkTag(rawTag);
      if (!tag || !selectedAssets.length || tagOperationRef.current) return false;
      if (selectedAssets.length === 1 && !assetTagState.get(selectedAssets[0]!.id)) return false;
      const capturedSelectionKey = selectionKey;
      const capturedAssets = [...selectedAssets];
      const operationGeneration = tagOperationGenerationRef.current + 1;
      tagOperationGenerationRef.current = operationGeneration;
      tagOperationRef.current = true;
      setTagApplying(true);
      setTagSaveFailed(false);

      if (capturedAssets.length === 1) {
        const assetId = capturedAssets[0]!.id;
        const baseTags = assetTagState.get(assetId)?.tags;
        if (!baseTags) return false;
        const nextTags = mergeTagLists(baseTags, [tag]);
        if (nextTags.length === baseTags.length) {
          tagOperationRef.current = false;
          setTagApplying(false);
          return true;
        }
        const mutationToken = assetTagState.beginMutation(assetId);
        if (!mutationToken) {
          if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
          if (tagOperationGenerationRef.current === operationGeneration) {
            tagOperationRef.current = false;
            setTagApplying(false);
          }
          return false;
        }
        try {
          const result = await setAssetTags(assetId, nextTags);
          if (!assetTagState.settleMutation(mutationToken, result.tags)) return false;
          const cached = getCachedDetail(detailsCacheRef.current, assetId, assetTagState.epoch);
          if (cached) {
            putCachedDetail(
              detailsCacheRef.current,
              DETAILS_CACHE_LIMIT,
              { ...cached, tags: result.tags },
              assetTagState.epoch
            );
          }
          if (selectionKeyRef.current === capturedSelectionKey) setSingleAssetTags(result.tags);
          void refreshKnownTags().catch(() => []);
          if (bulkTagMutationRequiresRefresh(result.changed ? 1 : 0, appliedFilterTags)) {
            void refresh().catch(() => {});
          }
          return true;
        } catch {
          assetTagState.settleMutation(mutationToken);
          if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
          return false;
        } finally {
          if (tagOperationGenerationRef.current === operationGeneration) {
            tagOperationRef.current = false;
            setTagApplying(false);
          }
        }
      }

      const assetIds = capturedAssets.map((asset) => asset.id);
      const mutationTokens = new Map<number, NonNullable<ReturnType<typeof assetTagState.beginMutation>>>();
      for (const assetId of assetIds) {
        const token = assetTagState.beginMutation(assetId);
        if (!token) {
          for (const acquiredToken of mutationTokens.values()) {
            assetTagState.settleMutation(acquiredToken);
          }
          if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
          if (tagOperationGenerationRef.current === operationGeneration) {
            tagOperationRef.current = false;
            setTagApplying(false);
          }
          return false;
        }
        mutationTokens.set(assetId, token);
      }
      try {
        const result = await applyBulkTagsAction({ assetIds, tags: [tag], refreshKnownTags });
        if (!result || result.processed_assets === 0) {
          for (const token of mutationTokens.values()) assetTagState.settleMutation(token);
          void refresh().catch(() => {});
          return false;
        }
        let acceptedResults = 0;
        const returnedIds = new Set<number>();
        for (const item of result.results) {
          const token = mutationTokens.get(item.asset_id);
          if (!token) continue;
          returnedIds.add(item.asset_id);
          if (!assetTagState.settleMutation(token, item.tags)) continue;
          acceptedResults += 1;
          const cached = getCachedDetail(detailsCacheRef.current, item.asset_id, assetTagState.epoch);
          if (cached) {
            putCachedDetail(
              detailsCacheRef.current,
              DETAILS_CACHE_LIMIT,
              { ...cached, tags: item.tags },
              assetTagState.epoch
            );
          }
        }
        for (const [assetId, token] of mutationTokens) {
          if (!returnedIds.has(assetId)) assetTagState.settleMutation(token);
        }
        if (acceptedResults === 0) return false;
        if (selectionKeyRef.current === capturedSelectionKey) {
          setAppliedBulkTags((previous) => mergeTagLists(previous, [tag]));
        }
        if (result.processed_assets !== assetIds.length) void refresh().catch(() => {});
        else if (bulkTagMutationRequiresRefresh(result.updated_assets, appliedFilterTags)) {
          void refresh().catch(() => {});
        }
        return true;
      } catch {
        for (const token of mutationTokens.values()) assetTagState.settleMutation(token);
        if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
        return false;
      } finally {
        if (tagOperationGenerationRef.current === operationGeneration) {
          tagOperationRef.current = false;
          setTagApplying(false);
        }
      }
    },
    [appliedFilterTags, assetTagState, refresh, refreshKnownTags, selectedAssets, selectionKey, setAssets]
  );

  const onRemoveTag = useCallback(
    async (tag: string) => {
      if (selectedAssets.length !== 1 || tagOperationRef.current) return;
      const capturedSelectionKey = selectionKey;
      const assetId = selectedAssets[0]!.id;
      const baseTags = assetTagState.get(assetId)?.tags;
      if (!baseTags) return;
      const nextTags = baseTags.filter((item) => item !== tag);
      const operationGeneration = tagOperationGenerationRef.current + 1;
      tagOperationGenerationRef.current = operationGeneration;
      tagOperationRef.current = true;
      setTagApplying(true);
      setTagSaveFailed(false);
      const mutationToken = assetTagState.beginMutation(assetId);
      if (!mutationToken) {
        if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
        if (tagOperationGenerationRef.current === operationGeneration) {
          tagOperationRef.current = false;
          setTagApplying(false);
        }
        return;
      }
      try {
        const result = await setAssetTags(assetId, nextTags);
        if (!assetTagState.settleMutation(mutationToken, result.tags)) return;
        const cached = getCachedDetail(detailsCacheRef.current, assetId, assetTagState.epoch);
        if (cached) {
          putCachedDetail(
            detailsCacheRef.current,
            DETAILS_CACHE_LIMIT,
            { ...cached, tags: result.tags },
            assetTagState.epoch
          );
        }
        if (selectionKeyRef.current === capturedSelectionKey) setSingleAssetTags(result.tags);
        void refreshKnownTags().catch(() => []);
        if (bulkTagMutationRequiresRefresh(result.changed ? 1 : 0, appliedFilterTags)) {
          void refresh().catch(() => {});
        }
      } catch {
        assetTagState.settleMutation(mutationToken);
        if (selectionKeyRef.current === capturedSelectionKey) setTagSaveFailed(true);
      } finally {
        if (tagOperationGenerationRef.current === operationGeneration) {
          tagOperationRef.current = false;
          setTagApplying(false);
        }
      }
    },
    [appliedFilterTags, assetTagState, refresh, refreshKnownTags, selectedAssets, selectionKey, setAssets]
  );

  const onRetryTagDetails = useCallback(() => {
    if (selectedAssets.length !== 1 || tagDetailsLoading) return;
    setTagDetailsRetry((value) => value + 1);
  }, [selectedAssets.length, tagDetailsLoading]);

  return {
    selectionModeEnabled,
    favoriteApplying,
    favoriteFailed: favoriteFailedSelection === selectedAssetIds,
    allSelectedFavorites: selectedAssetIds.size > 0 && selectedAssets.length === selectedAssetIds.size
      && selectedAssets.every((asset) => asset.is_favorite),
    onToggleFavorite,
    selectedAssetIds,
    selectedAssets,
    knownTags,
    groupKeyDraft,
    orderedAssetIds,
    hasConflictingGroups,
    groupApplying,
    groupFailed,
    tagMode: selectedAssets.length === 0 ? "none" as const : selectedAssets.length === 1 ? "single" as const : "multiple" as const,
    singleAssetTags,
    appliedBulkTags,
    tagDetailsLoading,
    tagDetailsFailed,
    tagApplying,
    tagSaveFailed,
    onToggleSelectionMode,
    onBulkSelectionInteraction,
    onGroupKeyDraftChange: setGroupKeyDraft,
    onReorderGroupAsset: (draggedAssetId: number, targetAssetId: number) =>
      setOrderedAssetIds((previous) => reorderAssetIdsByDrop(previous, draggedAssetId, targetAssetId)),
    onApplyGroup,
    onAddTag,
    onRemoveTag,
    onRetryTagDetails
  };
}
