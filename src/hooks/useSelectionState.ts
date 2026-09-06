import { useSelectionNavigation } from "./useSelectionNavigation";
import { useMediaGroupDraft } from "./useMediaGroupDraft";
import type { FilterDescriptor } from "../components/app/services/filterService";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { deleteLightboxAssetAction } from "../components/lightbox/services/deleteLightboxAssetAction";
import { useSelectionMetadataActions } from "./useSelectionMetadataActions";
import { saveLightboxTagsAction } from "../components/lightbox/services/saveLightboxTagsAction";
import {
  collectChangedTags,
  tagMutationTouchesFilters
} from "../components/app/services/libraryInvalidationService";
import { useAssetTagState } from "../components/app/hooks/useAssetTagState";
import type {
  AssetTagGenerationToken,
  AssetTagMutationToken,
  AssetTagStateController
} from "../components/app/hooks/useAssetTagState";
import type { AssetDetails, AssetSummary, SelectedAsset } from "../types";
import { getAssetDetails } from "../api";
import { normalizeTags } from "../utils/media";
import { useTranslation } from "react-i18next";

interface UseSelectionStateArgs {
  assets: AssetSummary[];
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  appliedFavoritesOnly: boolean;
  refresh: () => Promise<void>;
  refreshKnownTags: () => Promise<string[]>;
  assetTagState?: AssetTagStateController;
  assetCount?: number;
  queryEpoch?: number;
  getAssetAtAsync?: (index: number) => Promise<AssetSummary | undefined>;
  getAssetIndex?: (assetId: number) => number | null;
  appliedFilter?: FilterDescriptor;
  appliedFilterTags?: string[];
}

interface TagMutationState {
  desired: string[] | null;
  confirmed: string[];
  inFlight: boolean;
  failed: boolean;
  token: AssetTagMutationToken | null;
}

interface CachedDetail {
  generation: AssetTagGenerationToken;
  details: AssetDetails;
  epoch: number;
}

const DETAILS_CACHE_LIMIT = 256;

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

function detailFromView(view: SelectedAsset, tags: string[]): AssetDetails | null {
  if (view.path === null || view.size_bytes === null) return null;
  return { ...view, path: view.path, size_bytes: view.size_bytes, tags };
}

/** Summary-backed view before details arrive: GIF/video may start from the
 * valid summary source path; images wait for the detail read instead of
 * guessing a source from the file name. A carried full path/size (from an
 * already detail-complete selection object) is preserved as-is. */
function summaryToView(summary: AssetSummary, tags: string[]): SelectedAsset {
  const carried = summary as Partial<SelectedAsset>;
  return {
    ...summary,
    path: carried.path ?? summary.preview_path,
    size_bytes: carried.size_bytes ?? null,
    tags
  };
}

export function useSelectionState({
  assets,
  setAssets,
  appliedFavoritesOnly,
  refresh,
  refreshKnownTags,
  assetTagState: sharedAssetTagState,
  assetCount = assets.length,
  queryEpoch = 0,
  getAssetAtAsync = async (index) => assets[index],
  getAssetIndex = (assetId) => {
    const index = assets.findIndex((asset) => asset.id === assetId);
    return index >= 0 ? index : null;
  },
  appliedFilter,
  appliedFilterTags = []
}: UseSelectionStateArgs) {
  const { t } = useTranslation();
  const localAssetTagState = useAssetTagState();
  const assetTagState = sharedAssetTagState ?? localAssetTagState;
  const [selected, setSelectedState] = useState<SelectedAsset | null>(null);
  const syncedSummaryRef = useRef<AssetSummary | null>(null);
  const selectedRef = useRef<SelectedAsset | null>(null);
  selectedRef.current = selected;
  const selectionRequestRef = useRef(0);
  const navigationRequestRef = useRef(0);
  const selectedIndexRef = useRef<number | null>(null);
  const navigationTargetIndexRef = useRef<number | null>(null);
  const favoriteChangeRevisionRef = useRef(0);
  const detailsCacheRef = useRef<Map<number, CachedDetail>>(new Map());
  const tagMutationsRef = useRef<Map<number, TagMutationState>>(new Map());
  const observedTagEpochRef = useRef(assetTagState.epoch);
  const observedQueryEpochRef = useRef(queryEpoch);
  const skipAssetSyncForEpochRef = useRef<number | null>(null);
  const [tagEditor, setTagEditor] = useState<string[]>([]);
  const [tagSaving, setTagSaving] = useState(false);
  const [tagFailed, setTagFailed] = useState(false);
  const [tagDetailsLoading, setTagDetailsLoading] = useState(false);
  const [tagDetailsFailed, setTagDetailsFailed] = useState(false);
  const [assetDetailsFailed, setAssetDetailsFailed] = useState(false);
  const { mediaGroupKeyEditor, setMediaGroupKeyEditor, mediaGroupOrderEditor, setMediaGroupOrderEditor } = useMediaGroupDraft(selected);
  const selectedId = selected?.id;
  useEffect(() => selectedId === undefined ? undefined : assetTagState.pin(selectedId), [assetTagState.pin, selectedId]);
  useEffect(() => () => { selectionRequestRef.current++; navigationRequestRef.current++; }, []);
  useEffect(() => () => {
    if (selectedId === undefined) return;
    const mutation = tagMutationsRef.current.get(selectedId);
    if (mutation && !mutation.inFlight && !mutation.desired && !mutation.failed) tagMutationsRef.current.delete(selectedId);
  }, [selectedId]);

  const getCachedDetails = useCallback((assetId: number): AssetDetails | null => {
    const entry = detailsCacheRef.current.get(assetId);
    if (!entry) return null;
    if (entry.epoch !== assetTagState.epoch || !assetTagState.isCurrent(entry.generation)) {
      detailsCacheRef.current.delete(assetId);
      return null;
    }
    detailsCacheRef.current.delete(assetId);
    detailsCacheRef.current.set(assetId, entry);
    return entry.details;
  }, [assetTagState]);

  const putCachedDetails = useCallback((details: AssetDetails) => {
    detailsCacheRef.current.delete(details.id);
    detailsCacheRef.current.set(details.id, { details, epoch: assetTagState.epoch, generation: assetTagState.captureGeneration(details.id) });
    while (detailsCacheRef.current.size > DETAILS_CACHE_LIMIT) {
      const oldest = detailsCacheRef.current.keys().next();
      if (oldest.done) break;
      detailsCacheRef.current.delete(oldest.value);
    }
  }, [assetTagState]);

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
    setAssetDetailsFailed(false);

  }, [assetTagState.epoch]);

  // A new query session reorders the result list: recompute the lightbox
  // position from the fresh snapshot, or invalidate it when the selected
  // record is not resolvable (it fell out of the active filters).
  useEffect(() => {
    if (observedQueryEpochRef.current === queryEpoch) return;
    observedQueryEpochRef.current = queryEpoch;
    // A new session invalidates cached details: rows may have changed on disk
    // between sessions even when IDs are reused.
    detailsCacheRef.current.clear();
    const current = selectedRef.current;
    if (!current) return;
    const index = getAssetIndex(current.id);
    selectedIndexRef.current = index;
    navigationTargetIndexRef.current = index;
  }, [getAssetIndex, queryEpoch]);

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
    const cachedDetails = getCachedDetails(assetId);
    const selectedSnapshot =
      selectedRef.current?.id === assetId
        ? { ...selectedRef.current, tags: authoritative.tags }
        : cachedDetails
          ? { ...cachedDetails, tags: authoritative.tags }
          : (() => {
              const summary = assets.find((asset) => asset.id === assetId);
              return summary ? summaryToView(summary, authoritative.tags) : null;
            })();

    let mutationSettled = false;
    void saveLightboxTagsAction(
      {
        selected: selectedSnapshot,
        tagEditor: requestedTags,
        setSelected: setSelectedState,
        refreshKnownTags,
        onSaved: (savedAssetId, tags) => {
          const accepted = assetTagState.settleMutation(mutationToken, tags);
          mutationSettled = accepted;
          if (!accepted) return false;
          const base = getCachedDetails(savedAssetId) ??
            (selectedRef.current?.id === savedAssetId ? selectedRef.current : selectedSnapshot);
          // Only a detail-complete base may enter the details cache.
          const completeBase = base ? detailFromView(base, tags) : null;
          if (completeBase) putCachedDetails(completeBase);
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
            appliedFilter ?? appliedFilterTags
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
        else if (!current.failed && selectedRef.current?.id !== assetId) tagMutationsRef.current.delete(assetId);
      });
  }, [
    appliedFilter, appliedFilterTags, assetTagState, assets, getCachedDetails, putCachedDetails, refresh,
    refreshKnownTags, syncVisibleTagMutationState
  ]);

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
      setAssetDetailsFailed(false);

      return;
    }

    const mutation = tagMutationsRef.current.get(selected.id);
    const authoritative = assetTagState.get(selected.id);
    setTagEditor(mutation?.desired ?? authoritative?.tags ?? []);
    setTagSaving(Boolean(mutation?.inFlight));
    setTagFailed(Boolean(mutation?.failed));

  }, [assetTagState, selected]);

  useEffect(() => {
    const current = selectedRef.current;
    if (!current) return;
    const authoritative = assetTagState.get(current.id);
    if (!authoritative) return;
    setTagDetailsLoading(false);
    setTagDetailsFailed(false);
    const cached = getCachedDetails(current.id);
    if (cached && !sameTags(cached.tags, authoritative.tags)) {
      putCachedDetails({ ...cached, tags: authoritative.tags });
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
  }, [assetTagState, getCachedDetails, putCachedDetails]);

  useEffect(() => {
    if (skipAssetSyncForEpochRef.current === assetTagState.epoch) {
      skipAssetSyncForEpochRef.current = null;
      return;
    }
    if (selectedId === undefined) return;
    const latest = assets.find((asset) => asset.id === selectedId);
    if (!latest) {
      if (assets.length === 0) setSelectedState(null);
      return;
    }

    if (syncedSummaryRef.current === latest) return;
    syncedSummaryRef.current = latest;
    setSelectedState((current) => {
      if (!current) return summaryToView(latest, []);
      const mutation = tagMutationsRef.current.get(current.id);
      const authoritative = assetTagState.get(current.id);
      const next = {
        ...latest,
        path: current.path ?? latest.preview_path,
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
  }, [assetTagState, assets, selectedId]);

  const { toggleSelectedFavorite, saveMediaGroup, favoritePending, groupPending, favoriteFailed, groupFailed } = useSelectionMetadataActions({
    selected, assetTagState, appliedFavoritesOnly, setAssets, setSelectedState, refresh, getCachedDetails, putCachedDetails
  });

  const applyFavoriteChanges = useCallback((assetIds: ReadonlySet<number>, isFavorite: boolean) => {
    favoriteChangeRevisionRef.current += 1;
    for (const assetId of assetIds) {
      const cached = getCachedDetails(assetId);
      if (cached) putCachedDetails({ ...cached, is_favorite: isFavorite });
    }
    const current = selectedRef.current;
    if (current && assetIds.has(current.id)) {
      selectedRef.current = { ...current, is_favorite: isFavorite };
      setSelectedState((previous) => previous && assetIds.has(previous.id)
        ? { ...previous, is_favorite: isFavorite } : previous);
    }
  }, [getCachedDetails, putCachedDetails]);

  const prefetchAdjacentDetails = useCallback((index: number) => {
    if (assetCount <= 1) return;
    const queryIdentity = observedQueryEpochRef.current;
    for (const adjacentIndex of [(index - 1 + assetCount) % assetCount, (index + 1) % assetCount]) {
      void getAssetAtAsync(adjacentIndex).then((adjacent) => {
        if (queryIdentity !== observedQueryEpochRef.current || !adjacent || getCachedDetails(adjacent.id)) return;
        const generation = assetTagState.captureGeneration(adjacent.id);
        const favoriteRevision = favoriteChangeRevisionRef.current;
        void getAssetDetails(adjacent.id).then((adjacentDetails) => {
          if (queryIdentity !== observedQueryEpochRef.current || !adjacentDetails || favoriteRevision !== favoriteChangeRevisionRef.current) return;
          const accepted = assetTagState.publishDetails(adjacent.id, adjacentDetails.tags, generation);
          const authoritative = assetTagState.get(adjacent.id);
          if (!accepted) return;
          const canonical = authoritative?.tags ?? adjacentDetails.tags;
          putCachedDetails({ ...adjacentDetails, tags: canonical });
        }).catch(() => {});
      }).catch(() => {});
    }
  }, [assetCount, assetTagState, getCachedDetails, getAssetAtAsync, putCachedDetails]);

  const selectAsset = useCallback((summary: AssetSummary | null, knownIndex?: number) => {
    navigationRequestRef.current += 1;
    const requestId = selectionRequestRef.current + 1;
    selectionRequestRef.current = requestId;
    if (!summary) {
      selectedIndexRef.current = null;
      navigationTargetIndexRef.current = null;
      setSelectedState(null);
      return;
    }
    syncedSummaryRef.current = summary;
    const selectedIndex = knownIndex ?? getAssetIndex(summary.id);
    selectedIndexRef.current = selectedIndex;
    navigationTargetIndexRef.current = selectedIndex;

    let authoritative = assetTagState.get(summary.id);
    // Non-empty tag arrays can only come from a full selection/detail object;
    // gallery summaries never carry tags.
    const carriedTags = (summary as Partial<SelectedAsset>).tags;
    if (!authoritative && Array.isArray(carriedTags) && carriedTags.length > 0) {
      const generation = assetTagState.captureGeneration(summary.id);
      assetTagState.publishDetails(summary.id, carriedTags, generation);
      authoritative = assetTagState.get(summary.id);
    }

    const cached = getCachedDetails(summary.id);
    if (cached) {
      let accepted = true;
      if (!authoritative) {
        const generation = assetTagState.captureGeneration(summary.id);
        accepted = assetTagState.publishDetails(summary.id, cached.tags, generation);
        authoritative = assetTagState.get(summary.id);
      }
      if (accepted || authoritative) {
        const mergedCached = { ...cached, tags: authoritative?.tags ?? cached.tags };
        putCachedDetails(mergedCached);
        setTagDetailsLoading(false);
        setTagDetailsFailed(false);
        setAssetDetailsFailed(false);
        setSelectedState(mergedCached);
        return;
      }
      detailsCacheRef.current.delete(summary.id);
    }

    setSelectedState(summaryToView(summary, authoritative?.tags ?? []));
    setTagDetailsLoading(!authoritative);
    setTagDetailsFailed(false);
    setAssetDetailsFailed(false);
    let detailGeneration = assetTagState.captureGeneration(summary.id);
    const favoriteRevision = favoriteChangeRevisionRef.current;
    const readCurrentDetails = async () => {
      let details = await getAssetDetails(summary.id);
      while (selectionRequestRef.current === requestId && !assetTagState.isCurrent(detailGeneration)) {
        await assetTagState.waitForIdle();
        if (selectionRequestRef.current !== requestId) return null;
        detailGeneration = assetTagState.captureGeneration(summary.id);
        details = await getAssetDetails(summary.id);
      }
      return details;
    };
    void readCurrentDetails().then((details) => {
      if (selectionRequestRef.current !== requestId) return;
      if (!details) {
        setAssetDetailsFailed(true);
        if (!assetTagState.get(summary.id)) setTagDetailsFailed(true);
        return;
      }
      const accepted = assetTagState.publishDetails(details.id, details.tags, detailGeneration);
      const authoritativeDetails = assetTagState.get(details.id);
      if (!accepted) {
        setAssetDetailsFailed(true);
        setTagDetailsFailed(true);
        return;
      }
      const canonical = authoritativeDetails?.tags ?? details.tags;
      const current = selectedRef.current;
      const favoriteChanged = favoriteRevision !== favoriteChangeRevisionRef.current && current?.id === details.id;
      const merged = {
        ...details,
        is_favorite: favoriteChanged ? current.is_favorite : details.is_favorite,
        tags: canonical
      };
      putCachedDetails(merged);
      setSelectedState(merged);
      setTagDetailsFailed(false);
      setAssetDetailsFailed(false);
      const index = selectedIndexRef.current;
      if (index !== null) prefetchAdjacentDetails(index);
    }).catch(() => {
      if (selectionRequestRef.current === requestId) {
        setAssetDetailsFailed(true);
        if (!assetTagState.get(summary.id)) setTagDetailsFailed(true);
      }
    }).finally(() => {
      if (selectionRequestRef.current === requestId) setTagDetailsLoading(false);
    });
  }, [
    assetTagState, getCachedDetails, getAssetIndex, prefetchAdjacentDetails, putCachedDetails
  ]);

  const retryTagDetails = useCallback(() => {
    const current = selectedRef.current;
    if (!current || (!assetDetailsFailed && !tagDetailsFailed)) return;
    selectAsset(current, selectedIndexRef.current ?? undefined);
  }, [assetDetailsFailed, selectAsset, tagDetailsFailed]);

  const { handleSelectPrevious, handleSelectNext } = useSelectionNavigation({ assetCount,
    getAssetAtAsync, selectAsset, selectedIndexRef, navigationTargetIndexRef, navigationRequestRef });

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
    assetDetailsFailed,
    saveTags,
    retryTags,
    retryTagDetails,
    mediaGroupKeyEditor,
    setMediaGroupKeyEditor,
    mediaGroupOrderEditor,
    setMediaGroupOrderEditor,
    selectAsset,
    saveMediaGroup,
    toggleSelectedFavorite, favoritePending, groupPending, favoriteFailed, groupFailed,
    applyFavoriteChanges,
    handleSelectPrevious,
    handleSelectNext,
    deleteSelectedAsset
  }), [
    deleteSelectedAsset, handleSelectNext, handleSelectPrevious, mediaGroupKeyEditor,
    mediaGroupOrderEditor, retryTagDetails, retryTags, saveMediaGroup, saveTags, selectAsset, selected,
    assetDetailsFailed, tagDetailsFailed, tagDetailsLoading, tagEditor, tagFailed, tagSaving,
    toggleSelectedFavorite, favoritePending, groupPending, favoriteFailed, groupFailed, applyFavoriteChanges, setMediaGroupKeyEditor, setMediaGroupOrderEditor
  ]);
}
