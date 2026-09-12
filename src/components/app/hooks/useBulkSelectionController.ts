import { useQueryInvalidation, type OnTagMutation } from "./useQueryInvalidation";
import type { BulkGroupSaveResult } from "../types";
import { useSelectedSummaries } from "./useSelectedSummaries";
import type { FilterDescriptor } from "../services/filterService";
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
import type { BulkSelectionInteraction, SelectionRange } from "../../gallery/selection";
import { containsIndex } from "../../gallery/services/selectionGeometry";

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
  startupPopularTags: string[];
  settingsViewOpen: boolean;
  queueThumbnailsByIds: (assetIds: number[]) => void;
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  assetTagState?: AssetTagStateController;
  onTagMutation?: OnTagMutation;
  appliedFilter?: FilterDescriptor;
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
  startupPopularTags,
  settingsViewOpen,
  queueThumbnailsByIds,
  setAssets,
  refresh: refreshQuery,
  refreshKnownTags,
  assetTagState: sharedAssetTagState,
  appliedFavoritesOnly = false,
  onFavoritesChanged,
  appliedFilter,
  onTagMutation: onCommitted,
  appliedFilterTags = []
}: UseBulkSelectionControllerOptions) {
  const { refresh, onTagMutation, isFavoritesOnly } = useQueryInvalidation(
    appliedFilter ?? appliedFilterTags, refreshQuery, appliedFavoritesOnly, onCommitted
  );
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
  const [selectionBusy, setSelectionBusy] = useState(false);
  const selectionBusyRef = useRef(false);
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const retrySelectionRef = useRef<(() => void) | null>(null);
  const invalidateSelectionRequest = useCallback(() => {
    rangeRequestRef.current++;
    selectionBusyRef.current = false;
    setSelectionBusy(false);
    setSelectionError(null);
    retrySelectionRef.current = null;
  }, []);
  useEffect(() => () => { rangeRequestRef.current++; }, []);
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
    invalidateSelectionRequest();
    tagOperationRef.current = false;
    groupOperationRef.current = false;
    setSelectedAssetIds(new Set());
    selectionAnchorIndexRef.current = null;
    setSingleAssetTags([]);
    setAppliedBulkTags([]);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    setTagApplying(false);
    setTagSaveFailed(false);
    setGroupApplying(false);
    setGroupFailed(false);
  }, [assetTagState.epoch, invalidateSelectionRequest]);

  const metadata = useSelectedSummaries(selectedAssetIds, assets, assetTagState.epoch);
  const selectedAssets = metadata.selected;
  const selectionIds = useMemo(() => [...selectedAssetIds], [selectedAssetIds]);
  const selectionKey = selectionIds.join(",");
  const groupInitializedRef = useRef("");
  const groupDirtyRef = useRef(false);
  const [partialResult, setPartialResult] = useState<{ processed: number; requested: number } | null>(null);
  const singleId = selectionIds.length === 1 ? selectionIds[0] : undefined;
  useEffect(() => singleId === undefined ? undefined : assetTagState.pin(singleId), [assetTagState.pin, singleId]);
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
    invalidateSelectionRequest();
  }, [queryEpoch, invalidateSelectionRequest]);


  useEffect(() => {
    groupInitializedRef.current = "";
    groupDirtyRef.current = false;
    setGroupKeyDraft("");
    setOrderedAssetIds(selectionIds);
    setHasConflictingGroups(false);
    setGroupFailed(false);
    setTagSaveFailed(false);
    setTagDetailsFailed(false);
    setAppliedBulkTags([]);
    setPartialResult(null);
  }, [selectionIds]);

  useEffect(() => {
    if (!metadata.ready || groupInitializedRef.current === selectionKey) return;
    groupInitializedRef.current = selectionKey;
    const derived = deriveBulkGroupSelectionState(selectedAssets);
    if (!groupDirtyRef.current) setGroupKeyDraft(derived.groupKey);
    setOrderedAssetIds(derived.orderedAssetIds);
    setHasConflictingGroups(derived.hasConflictingGroups);
  }, [metadata.ready, selectedAssets, selectionKey]);

  // Retry is an explicit user request to repeat this read.
  // biome-ignore lint/correctness/useExhaustiveDependencies: tagDetailsRetry deliberately repeats hydration without changing selection.
  useEffect(() => {
    if (skipDetailsForEpochRef.current === assetTagState.epoch) {
      skipDetailsForEpochRef.current = null;
      return;
    }
    const requestId = detailsRequestRef.current + 1;
    detailsRequestRef.current = requestId;
    if (!selectionModeEnabled || settingsViewOpen || selectionIds.length !== 1) {
      setSingleAssetTags([]);
      setTagDetailsLoading(false);
      setTagDetailsFailed(false);
      return;
    }

    const asset = { id: selectionIds[0]! };
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
  }, [assetTagState, selectionIds, selectionModeEnabled, settingsViewOpen, tagDetailsRetry]);

  useEffect(() => {
    if (selectionIds.length !== 1) return;
    const authoritative = assetTagState.get(selectionIds[0]!);
    if (!authoritative) return;
    setSingleAssetTags(authoritative.tags);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
  }, [assetTagState, selectionIds]);

  const resolveSelectionRanges = useCallback(async function resolve(
    ranges: SelectionRange[], additive: boolean, clearAnchor: boolean
  ): Promise<void> {
    invalidateSelectionRequest();
    const requestId = rangeRequestRef.current;
    const requestEpoch = queryEpoch;
    selectionBusyRef.current = true;
    setSelectionBusy(true);
    const isCurrent = () => rangeRequestRef.current === requestId && observedQueryEpochRef.current === requestEpoch;
    try {
      const resolved = new Set<number>();
      let rangeIndex = 0;
      let start = ranges[0]?.startIndex ?? 0;
      const lastIndex = ranges[ranges.length - 1]?.endIndex ?? -1;
      while (start <= lastIndex) {
        if (!isCurrent()) return;
        if (!getIdsRangeAsync) throw new Error("Selection range loader is unavailable");
        // Read each 128-index window once, including gaps between selected columns.
        // Resolving each row separately would repeatedly fetch the same uncached page.
        const end = Math.min(lastIndex, (Math.floor(start / 128) + 1) * 128 - 1);
        const ids = await getIdsRangeAsync(start, end);
        if (!isCurrent()) return;
        if (ids.length !== end - start + 1) throw new Error("Selection range is incomplete");
        for (let offset = 0; offset < ids.length; offset++) {
          if (containsIndex(ranges, start + offset)) resolved.add(ids[offset]);
        }
        while (rangeIndex < ranges.length && ranges[rangeIndex].endIndex <= end) rangeIndex++;
        start = Math.max(end + 1, ranges[rangeIndex]?.startIndex ?? lastIndex + 1);
      }
      if (!isCurrent()) return;
      setSelectedAssetIds(previous => new Set(additive ? [...previous, ...resolved] : resolved));
      if (clearAnchor) selectionAnchorIndexRef.current = null;
      return;
    } catch (error) {
      if (isCurrent()) {
        setSelectionError(String(error));
        retrySelectionRef.current = () => { void resolve(ranges, additive, clearAnchor); };
      }
      return;
    } finally {
      if (isCurrent()) {
        selectionBusyRef.current = false;
        setSelectionBusy(false);
      }
    }
  }, [getIdsRangeAsync, invalidateSelectionRequest, queryEpoch]);

  const onBulkSelectionInteraction = useCallback(
    (interaction: BulkSelectionInteraction): void | Promise<void> => {
      if (!selectionModeEnabled) return;
      invalidateSelectionRequest();
      switch (interaction.type) {
        case "clear":
          setSelectedAssetIds(new Set());
          selectionAnchorIndexRef.current = null;
          return;
        case "rectangle-start":
          selectionBusyRef.current = true;
          setSelectionBusy(true);
          return;
        case "rectangle-cancel": return;
        case "rectangle-commit":
          return resolveSelectionRanges(interaction.ranges, interaction.additive, true);
        case "click": {
          const { assetId, assetIndex, ctrlLike, shift } = interaction;
          const anchor = selectionAnchorIndexRef.current;
          if (shift && anchor !== null && getIdsRangeAsync) {
            return resolveSelectionRanges([{ startIndex: Math.min(anchor, assetIndex), endIndex: Math.max(anchor, assetIndex) }], ctrlLike, false);
          }
          selectionAnchorIndexRef.current = assetIndex;
          setSelectedAssetIds(previous => {
            if (shift && previous.has(assetId)) return previous;
            const next = new Set(previous);
            if (!shift && next.has(assetId)) next.delete(assetId);
            else next.add(assetId);
            return next;
          });
          return;
        }
        default: {
          const exhaustive: never = interaction;
          return exhaustive;
        }
      }
    },
    [getIdsRangeAsync, invalidateSelectionRequest, resolveSelectionRanges, selectionModeEnabled]
  );

  const onRetrySelection = useCallback(() => retrySelectionRef.current?.(), []);

  const onToggleSelectionMode = useCallback(() => {
    invalidateSelectionRequest();
    setSelectionModeEnabled(previous => !previous);
  }, [invalidateSelectionRequest]);

  const onToggleFavorite = useCallback(async () => {
    if (selectionBusyRef.current || selectedAssetIds.size === 0 || favoriteOperationRef.current) return;
    const capturedIds = selectedAssetIds;
    const capturedSelectionKey = selectionKey;
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
      if (selectionKeyRef.current === capturedSelectionKey) setPartialResult({ processed: acceptedIds.size, requested: capturedIds.size });
      metadata.patch(asset => acceptedIds.has(asset.id) ? { ...asset, is_favorite: result.is_favorite } : asset);
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
      if (missingIds.size > 0 || (acceptedIds.size > 0 && isFavoritesOnly())) {
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
  }, [isFavoritesOnly, assetTagState, onFavoritesChanged, refresh, selectedAssetIds, selectionKey, setAssets, metadata.patch]);

  const onApplyGroup = useCallback(async (order?: number[]): Promise<BulkGroupSaveResult> => {
    if (selectionBusyRef.current || !selectedAssetIds.size || !metadata.ready || groupOperationRef.current) return { status: "ignored" };
    const selectedIdSet = selectedAssetIds;
    const capturedOrderedIds = order ?? orderedAssetIds;
    // An explicit modal draft must still describe the complete current selection.
    if (capturedOrderedIds.length !== selectedIdSet.size ||
        new Set(capturedOrderedIds).size !== selectedIdSet.size ||
        capturedOrderedIds.some(id => !selectedIdSet.has(id))) return { status: "ignored" };
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
        return { status: "failed" };
      }
      mutationTokens.push(token);
    }

    groupOperationRef.current = true;
    setGroupApplying(true);
    setGroupFailed(false);
    try {
      const result = await applyBulkMediaGroupAction({
        assetIdsInOrder: capturedOrderedIds,
        groupKey: normalizedDraft,
        preservedSingleOrder: preservesExistingGroup ? singleAsset?.media_group_order : null,
        setAssets: updater => {
          if (mutationTokens.every(token => assetTagState.captureGeneration(token.assetId).epoch === token.epoch)) {
            setAssets(updater);
          }
        }
      });

      if (mutationTokens.some(token => assetTagState.captureGeneration(token.assetId).epoch !== token.epoch)) {
        return { status: "ignored" };
      }
      const processed = new Set(result?.processed_asset_ids ?? []);
      if (selectionKeyRef.current === capturedSelectionKey) setPartialResult({ processed: processed.size, requested: capturedOrderedIds.length });
      const normalizedKey = normalizedDraft || null;
      const orderById = new Map(capturedOrderedIds.map((id, index) => [id, normalizedKey ? preservesExistingGroup ? singleAsset?.media_group_order ?? 1 : index + 1 : null]));
      metadata.patch(asset => processed.has(asset.id) ? { ...asset, media_group_key: normalizedKey, media_group_order: orderById.get(asset.id) ?? null } : asset);
      capturedOrderedIds.forEach((assetId, index) => {
        if (!processed.has(assetId)) return;
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
        setOrderedAssetIds(capturedOrderedIds);
        setGroupKeyDraft(normalizedDraft);
        setHasConflictingGroups(false);
      }
      // Group changes alter ordering/adjacency of the active view, so restart
      // the query session instead of trusting the local patch alone.
      void refresh().catch(() => {});
      if (selectionKeyRef.current !== capturedSelectionKey) return { status: "ignored" };
      return processed.size === capturedOrderedIds.length
        ? { status: "saved" }
        : { status: "partial", processed: processed.size, requested: capturedOrderedIds.length };
    } catch {
      if (selectionKeyRef.current === capturedSelectionKey) setGroupFailed(true);
      return { status: "failed" };
    } finally {
      for (const token of mutationTokens) assetTagState.settleMutation(token);
      groupOperationRef.current = false;
      setGroupApplying(false);
    }
  }, [assetTagState, groupKeyDraft, orderedAssetIds, refresh, selectedAssets, selectionKey, setAssets, metadata.patch, metadata.ready, selectedAssetIds]);

  const onAddTag = useCallback(
    async (rawTag: string): Promise<boolean> => {
      const tag = normalizeBulkTag(rawTag);
      if (selectionBusyRef.current || !tag || !selectionIds.length || tagOperationRef.current) return false;
      if (selectionIds.length === 1 && !assetTagState.get(selectionIds[0]!)) return false;
      const capturedSelectionKey = selectionKey;
      const capturedIds = [...selectionIds];
      const operationGeneration = tagOperationGenerationRef.current + 1;
      tagOperationGenerationRef.current = operationGeneration;
      tagOperationRef.current = true;
      setTagApplying(true);
      setTagSaveFailed(false);

      if (capturedIds.length === 1) {
        const assetId = capturedIds[0]!;
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
          onTagMutation(result.query_impact);
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

      const assetIds = capturedIds;
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
          let accepted = false;
          for (const token of mutationTokens.values()) accepted = assetTagState.settleMutation(token) || accepted;
          if (!accepted) return false;
          if (selectionKeyRef.current === capturedSelectionKey) {
            setPartialResult({ processed: 0, requested: assetIds.length });
            setTagSaveFailed(true);
          }
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
        if (selectionKeyRef.current === capturedSelectionKey) setPartialResult({ processed: acceptedResults, requested: assetIds.length });
        if (acceptedResults === 0) return false;
        if (selectionKeyRef.current === capturedSelectionKey) {
          setAppliedBulkTags((previous) => mergeTagLists(previous, [tag]));
        }
        onTagMutation(result.query_impact, result.processed_assets !== assetIds.length);
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
    [onTagMutation, assetTagState, refresh, refreshKnownTags, selectionIds, selectionKey]
  );

  const onRemoveTag = useCallback(
    async (tag: string) => {
      if (selectionBusyRef.current) return;
      if (selectionIds.length !== 1 || tagOperationRef.current) return;
      const capturedSelectionKey = selectionKey;
      const assetId = selectionIds[0]!;
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
        onTagMutation(result.query_impact);
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
    [onTagMutation, assetTagState, refreshKnownTags, selectionIds, selectionKey]
  );

  const onRetryTagDetails = useCallback(() => {
    if (selectionIds.length !== 1 || tagDetailsLoading) return;
    setTagDetailsRetry((value) => value + 1);
  }, [selectionIds.length, tagDetailsLoading]);

  return {
    selectionQueryEpoch: queryEpoch,
    selectionBusy,
    selectionError,
    onRetrySelection,
    selectionModeEnabled,
    favoriteApplying,
    favoriteFailed: favoriteFailedSelection === selectedAssetIds,
    allSelectedFavorites: selectedAssetIds.size > 0 && selectedAssets.length === selectedAssetIds.size
      && selectedAssets.every((asset) => asset.is_favorite),
    onToggleFavorite,
    selectedAssetIds,
    selectedAssets,
    metadataLoading: !metadata.ready && !metadata.failed,
    metadataFailed: metadata.failed,
    onRetryMetadata: metadata.retry,
    queueThumbnailsByIds,
    partialResult,
    knownTags,
    startupPopularTags,
    groupKeyDraft,
    orderedAssetIds,
    hasConflictingGroups,
    groupApplying,
    groupFailed,
    tagMode: selectionIds.length === 0 ? "none" as const : selectionIds.length === 1 ? "single" as const : "multiple" as const,
    singleAssetTags,
    appliedBulkTags,
    tagDetailsLoading,
    tagDetailsFailed,
    tagApplying,
    tagSaveFailed,
    onToggleSelectionMode,
    onBulkSelectionInteraction,
    onGroupKeyDraftChange: (value: string) => { groupDirtyRef.current = true; setGroupKeyDraft(value); },
    onReorderGroupAsset: (draggedAssetId: number, targetAssetId: number) =>
      setOrderedAssetIds((previous) => reorderAssetIdsByDrop(previous, draggedAssetId, targetAssetId)),
    onApplyGroup,
    onAddTag,
    onRemoveTag,
    onRetryTagDetails
  };
}
