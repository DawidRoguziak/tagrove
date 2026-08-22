import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { deleteLightboxAssetAction } from "../components/lightbox/services/deleteLightboxAssetAction";
import { saveLightboxMediaGroupAction } from "../components/lightbox/services/saveLightboxMediaGroupAction";
import { saveLightboxTagsAction } from "../components/lightbox/services/saveLightboxTagsAction";
import { toggleLightboxFavoriteAction } from "../components/lightbox/services/toggleLightboxFavoriteAction";
import {
  collectChangedTags,
  tagMutationTouchesFilters
} from "../components/app/services/libraryInvalidationService";
import { useAssetTagState } from "../components/app/hooks/useAssetTagState";
import type {
  AssetTagMutationToken,
  AssetTagStateController
} from "../components/app/hooks/useAssetTagState";
import type { Asset } from "../types";
import { getAssetDetails } from "../api";
import { normalizeTags } from "../utils/media";
import { useTranslation } from "react-i18next";

interface UseSelectionStateArgs {
  assets: Asset[];
  setAssets: Dispatch<SetStateAction<Asset[]>>;
  appliedFavoritesOnly: boolean;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  assetTagState?: AssetTagStateController;
  assetCount?: number;
  getAssetAtAsync?: (index: number) => Promise<Asset | undefined>;
  getAssetIndex?: (assetId: number) => number | null;
  appliedFilterTags?: string[];
}

interface TagMutationState {
  desired: string[] | null;
  confirmed: string[];
  inFlight: boolean;
  failed: boolean;
  token: AssetTagMutationToken | null;
}

function sameTags(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index]);
}

function rebaseTagIntent(confirmed: string[], desired: string[], canonical: string[]): string[] {
  const confirmedSet = new Set(confirmed);
  const desiredSet = new Set(desired);
  const removed = new Set(confirmed.filter((tag) => !desiredSet.has(tag)));
  const additions = desired.filter((tag) => !confirmedSet.has(tag));
  return normalizeTags([
    ...canonical.filter((tag) => !removed.has(tag)),
    ...additions
  ]);
}

export function useSelectionState({
  assets,
  setAssets,
  appliedFavoritesOnly,
  refresh,
  refreshKnownTags,
  assetTagState: sharedAssetTagState,
  assetCount = assets.length,
  getAssetAtAsync = async (index) => assets[index],
  getAssetIndex = (assetId) => {
    const index = assets.findIndex((asset) => asset.id === assetId);
    return index >= 0 ? index : null;
  },
  appliedFilterTags = []
}: UseSelectionStateArgs) {
  const { t } = useTranslation();
  const localAssetTagState = useAssetTagState();
  const assetTagState = sharedAssetTagState ?? localAssetTagState;
  const [selected, setSelectedState] = useState<Asset | null>(null);
  const selectedRef = useRef<Asset | null>(null);
  selectedRef.current = selected;
  const selectionRequestRef = useRef(0);
  const navigationRequestRef = useRef(0);
  const selectedIndexRef = useRef<number | null>(null);
  const navigationTargetIndexRef = useRef<number | null>(null);
  const detailsCacheRef = useRef<Map<number, Asset>>(new Map());
  const tagMutationsRef = useRef<Map<number, TagMutationState>>(new Map());
  const observedTagEpochRef = useRef(assetTagState.epoch);
  const skipAssetSyncForEpochRef = useRef<number | null>(null);
  const [tagEditor, setTagEditor] = useState<string[]>([]);
  const [tagSaving, setTagSaving] = useState(false);
  const [tagFailed, setTagFailed] = useState(false);
  const [tagDetailsLoading, setTagDetailsLoading] = useState(false);
  const [tagDetailsFailed, setTagDetailsFailed] = useState(false);
  const [mediaGroupKeyEditor, setMediaGroupKeyEditor] = useState("");
  const [mediaGroupOrderEditor, setMediaGroupOrderEditor] = useState("");

  useEffect(() => {
    if (observedTagEpochRef.current === assetTagState.epoch) return;
    observedTagEpochRef.current = assetTagState.epoch;
    skipAssetSyncForEpochRef.current = assetTagState.epoch;
    selectionRequestRef.current += 1;
    navigationRequestRef.current += 1;
    selectedIndexRef.current = null;
    navigationTargetIndexRef.current = null;
    detailsCacheRef.current.clear();
    tagMutationsRef.current.clear();
    setSelectedState(null);
    setTagEditor([]);
    setTagSaving(false);
    setTagFailed(false);
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    setMediaGroupKeyEditor("");
    setMediaGroupOrderEditor("");
  }, [assetTagState.epoch]);

  const syncVisibleTagMutationState = useCallback((assetId: number) => {
    if (selectedRef.current?.id !== assetId) return;
    const mutation = tagMutationsRef.current.get(assetId);
    setTagSaving(Boolean(mutation?.inFlight));
    setTagFailed(Boolean(mutation?.failed));
    if (mutation?.desired) setTagEditor(mutation.desired);
  }, []);

  const drainTagMutation = useCallback((assetId: number) => {
    const mutation = tagMutationsRef.current.get(assetId);
    if (!mutation || mutation.inFlight || !mutation.desired || mutation.failed) return;
    const authoritative = assetTagState.get(assetId);
    if (!authoritative) return;

    const requestedTags = mutation.desired;
    const confirmedTags = [...mutation.confirmed];
    const mutationToken = assetTagState.beginMutation(assetId);
    if (!mutationToken) {
      mutation.token = null;
      mutation.failed = true;
      mutation.inFlight = false;
      syncVisibleTagMutationState(assetId);
      return;
    }
    mutation.token = mutationToken;
    mutation.inFlight = true;
    syncVisibleTagMutationState(assetId);
    const selectedSnapshot =
      selectedRef.current?.id === assetId
        ? { ...selectedRef.current, tags: authoritative.tags }
        : detailsCacheRef.current.get(assetId) ?? assets.find((asset) => asset.id === assetId) ?? null;

    let mutationSettled = false;
    void saveLightboxTagsAction(
      {
        selected: selectedSnapshot,
        tagEditor: requestedTags,
        setAssets,
        setSelected: setSelectedState,
        refreshKnownTags,
        onSaved: (savedAssetId, tags) => {
          const accepted = assetTagState.settleMutation(mutationToken, tags);
          mutationSettled = accepted;
          if (!accepted) return false;
          const cached = detailsCacheRef.current.get(savedAssetId);
          const base = cached ?? (selectedRef.current?.id === savedAssetId ? selectedRef.current : selectedSnapshot);
          if (base) detailsCacheRef.current.set(savedAssetId, { ...base, tags });
          return true;
        }
      },
      requestedTags
    )
      .then((result) => {
        if (!mutationSettled) assetTagState.settleMutation(mutationToken);
        const current = tagMutationsRef.current.get(assetId);
        if (!current || current.token !== mutationToken) return;
        current.token = null;
        current.confirmed = result?.tags ?? requestedTags;
        current.failed = false;
        if (current.desired && sameTags(current.desired, requestedTags)) current.desired = null;
        // A tag change that can flip the active include/exclude filter
        // membership must restart the query session, not just patch the cache.
        if (
          tagMutationTouchesFilters(
            collectChangedTags(confirmedTags, result?.tags ?? requestedTags),
            appliedFilterTags
          )
        ) {
          void refresh().catch(() => {});
        }
      })
      .catch(() => {
        assetTagState.settleMutation(mutationToken);
        const current = tagMutationsRef.current.get(assetId);
        if (!current || current.token !== mutationToken) return;
        current.token = null;
        current.failed = true;
        if (!current.desired) current.desired = requestedTags;
      })
      .finally(() => {
        const current = tagMutationsRef.current.get(assetId);
        if (!current) return;
        current.inFlight = false;
        syncVisibleTagMutationState(assetId);
        if (!current.failed && current.desired) drainTagMutation(assetId);
      });
  }, [appliedFilterTags, assetTagState, assets, refresh, refreshKnownTags, setAssets, syncVisibleTagMutationState]);

  const saveTags = useCallback((nextTags?: string[]) => {
    const assetId = selectedRef.current?.id;
    if (assetId === undefined) return;
    const authoritative = assetTagState.get(assetId);
    if (!authoritative) return;
    const normalized = normalizeTags(nextTags ?? tagEditor);
    const mutation = tagMutationsRef.current.get(assetId) ?? {
      desired: null,
      confirmed: authoritative.tags,
      inFlight: false,
      failed: false,
      token: null
    };
    mutation.desired = normalized;
    mutation.failed = false;
    tagMutationsRef.current.set(assetId, mutation);
    setTagEditor(normalized);
    syncVisibleTagMutationState(assetId);
    drainTagMutation(assetId);
  }, [assetTagState, drainTagMutation, syncVisibleTagMutationState, tagEditor]);

  const retryTags = useCallback(() => {
    const assetId = selectedRef.current?.id;
    if (assetId === undefined || !assetTagState.get(assetId)) return;
    const mutation = tagMutationsRef.current.get(assetId);
    if (!mutation) return;
    if (!mutation.desired) mutation.desired = tagEditor;
    mutation.failed = false;
    syncVisibleTagMutationState(assetId);
    drainTagMutation(assetId);
  }, [assetTagState, drainTagMutation, syncVisibleTagMutationState, tagEditor]);

  useEffect(() => {
    if (!selected) {
      selectedIndexRef.current = null;
      setTagEditor([]);
      setTagSaving(false);
      setTagFailed(false);
      setTagDetailsLoading(false);
      setTagDetailsFailed(false);
      setMediaGroupKeyEditor("");
      setMediaGroupOrderEditor("");
      return;
    }

    const mutation = tagMutationsRef.current.get(selected.id);
    const authoritative = assetTagState.get(selected.id);
    setTagEditor(mutation?.desired ?? authoritative?.tags ?? []);
    setTagSaving(Boolean(mutation?.inFlight));
    setTagFailed(Boolean(mutation?.failed));
    setMediaGroupKeyEditor(selected.media_group_key ?? "");
    setMediaGroupOrderEditor(
      selected.media_group_order === null ? "" : String(selected.media_group_order)
    );
  }, [assetTagState, selected]);

  useEffect(() => {
    const current = selectedRef.current;
    if (!current) return;
    const authoritative = assetTagState.get(current.id);
    if (!authoritative) return;
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    const cached = detailsCacheRef.current.get(current.id);
    if (cached && !sameTags(cached.tags, authoritative.tags)) {
      detailsCacheRef.current.set(current.id, { ...cached, tags: authoritative.tags });
    }
    const mutation = tagMutationsRef.current.get(current.id);
    if (mutation?.desired && !mutation.inFlight) {
      mutation.desired = rebaseTagIntent(mutation.confirmed, mutation.desired, authoritative.tags);
      mutation.confirmed = authoritative.tags;
      setTagEditor(mutation.desired);
    } else {
      if (mutation) mutation.confirmed = authoritative.tags;
      if (!mutation?.desired) setTagEditor(authoritative.tags);
    }
    setSelectedState((previous) =>
      previous?.id === current.id && !sameTags(previous.tags, authoritative.tags)
        ? { ...previous, tags: authoritative.tags }
        : previous
    );
  }, [assetTagState.revision]);

  useEffect(() => {
    if (skipAssetSyncForEpochRef.current === assetTagState.epoch) {
      skipAssetSyncForEpochRef.current = null;
      return;
    }
    if (!selected) return;
    const latest = assets.find((asset) => asset.id === selected.id);
    if (!latest) {
      if (assets.length === 0) setSelectedState(null);
      return;
    }

    setSelectedState((current) => {
      if (!current) return latest;
      const mutation = tagMutationsRef.current.get(current.id);
      const authoritative = assetTagState.get(current.id);
      const next = {
        ...latest,
        path: current.path,
        size_bytes: current.size_bytes,
        tags: mutation?.desired ?? authoritative?.tags ?? current.tags
      };
      const unchanged =
        next.id === current.id && next.path === current.path && next.kind === current.kind &&
        next.size_bytes === current.size_bytes && next.modified_at === current.modified_at &&
        next.width === current.width && next.height === current.height &&
        next.duration_ms === current.duration_ms && next.thumb_path === current.thumb_path &&
        next.is_favorite === current.is_favorite &&
        next.media_group_key === current.media_group_key &&
        next.media_group_order === current.media_group_order && sameTags(next.tags, current.tags);
      return unchanged ? current : next;
    });
  }, [assetTagState, assets, selected?.id]);

  const toggleSelectedFavorite = useCallback(async () => {
    await toggleLightboxFavoriteAction({
      selected,
      appliedFavoritesOnly,
      setAssets,
      setSelected: setSelectedState,
      refresh
    });
  }, [appliedFavoritesOnly, refresh, selected, setAssets]);

  const saveMediaGroup = useCallback(async (next: { key: string | null; order: number | null }) => {
    await saveLightboxMediaGroupAction({ selected, setAssets, setSelected: setSelectedState }, next);
    // Media-group changes always affect grouping/ordering, so start a new
    // session instead of relying on the local patch.
    void refresh().catch(() => {});
  }, [refresh, selected, setAssets]);

  const prefetchAdjacentDetails = useCallback((index: number) => {
    if (assetCount <= 1) return;
    for (const adjacentIndex of [(index - 1 + assetCount) % assetCount, (index + 1) % assetCount]) {
      void getAssetAtAsync(adjacentIndex).then((adjacent) => {
        if (!adjacent || detailsCacheRef.current.has(adjacent.id)) return;
        const generation = assetTagState.captureGeneration(adjacent.id);
        void getAssetDetails(adjacent.id).then((adjacentDetails) => {
          if (!adjacentDetails) return;
          const accepted = assetTagState.publishDetails(adjacent.id, adjacentDetails.tags, generation);
          const authoritative = assetTagState.get(adjacent.id);
          if (!accepted && !authoritative) return;
          const canonical = authoritative?.tags ?? adjacentDetails.tags;
          detailsCacheRef.current.set(adjacentDetails.id, { ...adjacentDetails, tags: canonical });
        }).catch(() => {});
      }).catch(() => {});
    }
  }, [assetCount, assetTagState, getAssetAtAsync]);

  const selectAsset = useCallback((asset: Asset | null, knownIndex?: number) => {
    navigationRequestRef.current += 1;
    const requestId = selectionRequestRef.current + 1;
    selectionRequestRef.current = requestId;
    if (!asset) {
      selectedIndexRef.current = null;
      navigationTargetIndexRef.current = null;
      setSelectedState(null);
      return;
    }
    const selectedIndex = knownIndex ?? getAssetIndex(asset.id);
    selectedIndexRef.current = selectedIndex;
    navigationTargetIndexRef.current = selectedIndex;

    let authoritative = assetTagState.get(asset.id);
    // Non-empty tag arrays can only come from a full detail object; gallery summaries always use [].
    if (!authoritative && asset.tags.length > 0) {
      const generation = assetTagState.captureGeneration(asset.id);
      assetTagState.publishDetails(asset.id, asset.tags, generation);
      authoritative = assetTagState.get(asset.id);
    }

    const cached = detailsCacheRef.current.get(asset.id);
    if (cached) {
      let accepted = true;
      if (!authoritative) {
        const generation = assetTagState.captureGeneration(asset.id);
        accepted = assetTagState.publishDetails(asset.id, cached.tags, generation);
        authoritative = assetTagState.get(asset.id);
      }
      if (accepted || authoritative) {
        const mergedCached = { ...cached, tags: authoritative?.tags ?? cached.tags };
        detailsCacheRef.current.set(asset.id, mergedCached);
        setTagDetailsLoading(false);
        setTagDetailsFailed(false);
        setSelectedState(mergedCached);
        return;
      }
      detailsCacheRef.current.delete(asset.id);
    }

    setSelectedState({ ...asset, tags: authoritative?.tags ?? [] });
    setTagDetailsLoading(!authoritative);
    setTagDetailsFailed(false);
    const detailGeneration = assetTagState.captureGeneration(asset.id);
    void getAssetDetails(asset.id).then((details) => {
      if (selectionRequestRef.current !== requestId) return;
      if (!details) {
        if (!assetTagState.get(asset.id)) setTagDetailsFailed(true);
        return;
      }
      const accepted = assetTagState.publishDetails(details.id, details.tags, detailGeneration);
      const authoritativeDetails = assetTagState.get(details.id);
      if (!accepted && !authoritativeDetails) {
        setTagDetailsFailed(true);
        return;
      }
      const canonical = authoritativeDetails?.tags ?? details.tags;
      const merged = { ...details, tags: canonical };
      detailsCacheRef.current.set(details.id, merged);
      setSelectedState(merged);
      setTagDetailsFailed(false);
      const index = selectedIndexRef.current;
      if (index !== null) prefetchAdjacentDetails(index);
    }).catch(() => {
      if (selectionRequestRef.current === requestId && !assetTagState.get(asset.id)) {
        setTagDetailsFailed(true);
      }
    }).finally(() => {
      if (selectionRequestRef.current === requestId) setTagDetailsLoading(false);
    });
  }, [assetTagState, getAssetIndex, prefetchAdjacentDetails]);

  const retryTagDetails = useCallback(() => {
    const current = selectedRef.current;
    if (!current || assetTagState.get(current.id)) return;
    selectAsset(current, selectedIndexRef.current ?? undefined);
  }, [assetTagState, selectAsset]);

  const navigateBy = useCallback((step: -1 | 1) => {
    const currentIndex = navigationTargetIndexRef.current ?? selectedIndexRef.current;
    if (currentIndex === null || assetCount === 0) return;
    const targetIndex = (currentIndex + step + assetCount) % assetCount;
    navigationTargetIndexRef.current = targetIndex;
    const requestId = navigationRequestRef.current + 1;
    navigationRequestRef.current = requestId;
    void getAssetAtAsync(targetIndex).then((asset) => {
      if (navigationRequestRef.current === requestId && asset) selectAsset(asset, targetIndex);
    }).catch(() => {});
  }, [assetCount, getAssetAtAsync, selectAsset]);

  const handleSelectPrevious = useCallback(() => {
    navigateBy(-1);
  }, [navigateBy]);

  const handleSelectNext = useCallback(() => {
    navigateBy(1);
  }, [navigateBy]);

  const deleteSelectedAsset = useCallback(async () => {
    const deletionIdentity = selected ? assetTagState.captureGeneration(selected.id) : null;
    await deleteLightboxAssetAction({
      selected,
      setAssets,
      setSelected: setSelectedState,
      refresh,
      refreshKnownTags,
      onResult: (summary) => {
        if (summary.source_status === "missing") {
          window.alert(t("lightbox.deleteConfirm.sourceMissing"));
        } else if (summary.source_status === "cleanup_pending") {
          window.alert(
            t("lightbox.deleteConfirm.cleanupPending", {
              path: summary.recovery_path ?? ""
            })
          );
        }
      },
      onDeleted: (assetId) => {
        if (!deletionIdentity || !assetTagState.remove(assetId, deletionIdentity)) return false;
        selectionRequestRef.current += 1;
        navigationRequestRef.current += 1;
        selectedIndexRef.current = null;
        navigationTargetIndexRef.current = null;
        setSelectedState((current) => current?.id === assetId ? null : current);
        detailsCacheRef.current.delete(assetId);
        tagMutationsRef.current.delete(assetId);
        return true;
      }
    });
  }, [assetTagState, refresh, refreshKnownTags, selected, setAssets, t]);

  return useMemo(() => ({
    selected,
    setSelected: selectAsset,
    tagEditor,
    setTagEditor,
    tagSaving,
    tagFailed,
    tagDetailsLoading,
    tagDetailsFailed,
    saveTags,
    retryTags,
    retryTagDetails,
    mediaGroupKeyEditor,
    setMediaGroupKeyEditor,
    mediaGroupOrderEditor,
    setMediaGroupOrderEditor,
    selectAsset,
    saveMediaGroup,
    toggleSelectedFavorite,
    handleSelectPrevious,
    handleSelectNext,
    deleteSelectedAsset
  }), [
    deleteSelectedAsset, handleSelectNext, handleSelectPrevious, mediaGroupKeyEditor,
    mediaGroupOrderEditor, retryTagDetails, retryTags, saveMediaGroup, saveTags, selectAsset, selected,
    tagDetailsFailed, tagDetailsLoading, tagEditor, tagFailed, tagSaving, toggleSelectedFavorite
  ]);
}
