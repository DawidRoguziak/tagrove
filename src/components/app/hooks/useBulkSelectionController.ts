import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { getAssetDetails, setAssetTags } from "../../../api";
import type { Asset, AssetDetails } from "../../../types";
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
import { updateAssetTags } from "../services/assetMutationService";
import { useAssetTagState } from "./useAssetTagState";
import type { AssetTagStateController } from "./useAssetTagState";

interface UseBulkSelectionControllerOptions {
  assets: Asset[];
  knownTags: string[];
  settingsViewOpen: boolean;
  queueThumbnailsByIds: (assetIds: number[]) => void;
  setAssets: Dispatch<SetStateAction<Asset[]>>;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  assetTagState?: AssetTagStateController;
}

function normalizedGroupIdentity(value: string | null): string | null {
  const normalized = normalizeGroupKey(value ?? "");
  return normalized ? normalized.toLowerCase() : null;
}

export function useBulkSelectionController({
  assets,
  knownTags,
  settingsViewOpen,
  queueThumbnailsByIds,
  setAssets,
  refresh,
  refreshKnownTags,
  assetTagState: sharedAssetTagState
}: UseBulkSelectionControllerOptions) {
  const localAssetTagState = useAssetTagState();
  const assetTagState = sharedAssetTagState ?? localAssetTagState;
  const [selectionModeEnabled, setSelectionModeEnabled] = useState(false);
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<number>>(new Set());
  const [selectionAnchorId, setSelectionAnchorId] = useState<number | null>(null);
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
  const detailsCacheRef = useRef<Map<number, AssetDetails>>(new Map());
  const detailsRequestRef = useRef(0);
  const groupOperationRef = useRef(false);
  const tagOperationRef = useRef(false);
  const tagOperationGenerationRef = useRef(0);
  const observedTagEpochRef = useRef(assetTagState.epoch);
  const skipDetailsForEpochRef = useRef<number | null>(null);

  useEffect(() => {
    if (observedTagEpochRef.current === assetTagState.epoch) return;
    observedTagEpochRef.current = assetTagState.epoch;
    skipDetailsForEpochRef.current = assetTagState.epoch;
    detailsRequestRef.current += 1;
    detailsCacheRef.current.clear();
    tagOperationGenerationRef.current += 1;
    tagOperationRef.current = false;
    groupOperationRef.current = false;
    setSelectedAssetIds(new Set());
    setSelectionAnchorId(null);
    setSingleAssetTags([]);
    setAppliedBulkTags([]);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    setTagApplying(false);
    setTagSaveFailed(false);
    setGroupApplying(false);
    setGroupFailed(false);
  }, [assetTagState.epoch]);

  const assetIndexById = useMemo(() => {
    const indexMap = new Map<number, number>();
    for (let index = 0; index < assets.length; index += 1) {
      const asset = assets[index];
      if (asset) indexMap.set(asset.id, index);
    }
    return indexMap;
  }, [assets]);

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
    setSelectionAnchorId(null);
  }, [selectionModeEnabled]);

  useEffect(() => {
    const availableIds = new Set(assets.map((asset) => asset.id));
    setSelectedAssetIds((previous) => {
      if (!previous.size) return previous;
      const next = new Set<number>();
      for (const id of previous) {
        if (availableIds.has(id)) next.add(id);
      }
      return next.size === previous.size ? previous : next;
    });
    setSelectionAnchorId((previous) =>
      previous === null || availableIds.has(previous) ? previous : null
    );
  }, [assets]);

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

    const cached = detailsCacheRef.current.get(asset.id);
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
        detailsCacheRef.current.set(details.id, { ...details, tags });
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

      if (viaDrag) {
        setSelectedAssetIds((previous) => {
          if (previous.has(assetId)) return previous;
          const next = new Set(previous);
          next.add(assetId);
          return next;
        });
        setSelectionAnchorId((previous) => previous ?? assetId);
        return;
      }

      if (shift) {
        const anchorIndex = selectionAnchorId === null ? undefined : assetIndexById.get(selectionAnchorId);
        if (anchorIndex !== undefined) {
          const from = Math.min(anchorIndex, assetIndex);
          const to = Math.max(anchorIndex, assetIndex);
          const rangeAssetIds = assets.slice(from, to + 1).map((asset) => asset.id);
          setSelectedAssetIds((previous) => {
            const next = ctrlLike ? new Set(previous) : new Set<number>();
            for (const id of rangeAssetIds) next.add(id);
            return next;
          });
          return;
        }
      }

      setSelectionAnchorId(assetId);
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
    [assetIndexById, assets, selectionAnchorId, selectionModeEnabled]
  );

  const onToggleSelectionMode = useCallback(() => {
    setSelectionModeEnabled((previous) => !previous);
  }, []);

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
        const cached = detailsCacheRef.current.get(assetId);
        if (!cached) return;
        const order = normalizedKey
          ? capturedOrderedIds.length === 1 && preservesExistingGroup
            ? singleAsset?.media_group_order ?? 1
            : index + 1
          : null;
        detailsCacheRef.current.set(assetId, {
          ...cached,
          media_group_key: normalizedKey,
          media_group_order: order
        });
      });

      if (selectionKeyRef.current === capturedSelectionKey) {
        setGroupKeyDraft(normalizedDraft);
        setHasConflictingGroups(false);
      }
    } catch {
      if (selectionKeyRef.current === capturedSelectionKey) setGroupFailed(true);
    } finally {
      groupOperationRef.current = false;
      setGroupApplying(false);
    }
  }, [groupKeyDraft, orderedAssetIds, selectedAssets, selectionKey, setAssets]);

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
          setAssets((previous) => updateAssetTags(previous, assetId, result.tags));
          const cached = detailsCacheRef.current.get(assetId);
          if (cached) detailsCacheRef.current.set(assetId, { ...cached, tags: result.tags });
          if (selectionKeyRef.current === capturedSelectionKey) setSingleAssetTags(result.tags);
          void refreshKnownTags().catch(() => []);
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
          const cached = detailsCacheRef.current.get(item.asset_id);
          if (cached) detailsCacheRef.current.set(item.asset_id, { ...cached, tags: item.tags });
        }
        for (const [assetId, token] of mutationTokens) {
          if (!returnedIds.has(assetId)) assetTagState.settleMutation(token);
        }
        if (acceptedResults === 0) return false;
        if (selectionKeyRef.current === capturedSelectionKey) {
          setAppliedBulkTags((previous) => mergeTagLists(previous, [tag]));
        }
        if (result.processed_assets !== assetIds.length) void refresh().catch(() => {});
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
    [assetTagState, refresh, refreshKnownTags, selectedAssets, selectionKey, setAssets]
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
        setAssets((previous) => updateAssetTags(previous, assetId, result.tags));
        const cached = detailsCacheRef.current.get(assetId);
        if (cached) detailsCacheRef.current.set(assetId, { ...cached, tags: result.tags });
        if (selectionKeyRef.current === capturedSelectionKey) setSingleAssetTags(result.tags);
        void refreshKnownTags().catch(() => []);
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
    [assetTagState, refreshKnownTags, selectedAssets, selectionKey, setAssets]
  );

  const onRetryTagDetails = useCallback(() => {
    if (selectedAssets.length !== 1 || tagDetailsLoading) return;
    setTagDetailsRetry((value) => value + 1);
  }, [selectedAssets.length, tagDetailsLoading]);

  return {
    selectionModeEnabled,
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
