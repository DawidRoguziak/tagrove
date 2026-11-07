import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import type { createLibrary } from "../../library/createLibrary";
import type { createSelection } from "../gallery/createSelection";
import { createSelectedSummaries } from "./createSelectedSummaries";
import {
  deriveBulkGroupSelectionState,
  normalizeGroupKey
} from "./bulkGroupOrderService";
import {
  mergeTagLists,
  normalizeBulkTag
} from "./bulkTagMergeService";

export type BulkGroupSaveResult =
  | { status: "saved" | "failed" | "ignored" }
  | { status: "partial"; processed: number; requested: number };
type ReadState = "idle" | "loading" | "failed";
export function createBulk(
  library: ReturnType<typeof createLibrary>,
  selection: ReturnType<typeof createSelection>,
  settingsOpen: () => boolean
) {
  const metadata = createSelectedSummaries({
    ids: () => selection.ids,
    cached: () => library.query.assets,
    epoch: () => library.tags.epoch,
    subscribe: library.mutations.subscribe
  });
  const [groupDraft, setGroupDraft] = createSignal("");
  const [order, setOrder] = createSignal<number[]>([]);
  const [conflicting, setConflicting] = createSignal(false);
  const [appliedTags, setAppliedTags] = createSignal<string[]>([]);
  const [detailsState, setDetailsState] = createSignal<ReadState>("idle");
  const [retry, setRetry] = createSignal(0);
  const [tagApplying, setTagApplying] = createSignal(false);
  const [groupApplying, setGroupApplying] = createSignal(false);
  const [favoriteApplying, setFavoriteApplying] = createSignal(false);
  const [tagFailed, setTagFailed] = createSignal(false);
  const [groupFailed, setGroupFailed] = createSignal(false);
  const [favoriteFailed, setFavoriteFailed] = createSignal(false);
  const [partial, setPartial] = createSignal<{ processed: number; requested: number } | null>(null);
  let disposed = false;
  // Writes queue per asset in library.mutations; these only count pending UI state.
  let tagWrites = 0;
  let groupWrites = 0;
  let favoriteWrites = 0;
  let draft = "";
  let dirty = false;
  let initialized: ReadonlySet<number> | null = null;
  let observed = selection.capture().ids;
  let epoch = library.tags.epoch;
  function synchronize() {
    const current = selection.capture().ids;
    if (observed === current && epoch === library.tags.epoch) return;
    observed = current;
    epoch = library.tags.epoch;
    initialized = null;
    dirty = false;
    draft = "";
    setGroupDraft("");
    setOrder([...current]);
    setConflicting(false);
    setAppliedTags([]);
    setPartial(null);
    setTagFailed(false);
    setGroupFailed(false);
    setFavoriteFailed(false);
  }
  createEffect(() => [selection.ids, library.tags.epoch], synchronize);
  createEffect(
    () => ({ ready: metadata.ready, assets: metadata.selected, ids: selection.ids }),
    ({ ready, assets, ids }) => {
      synchronize();
      if (!ready || initialized === ids) return;
      initialized = ids;
      const derived = deriveBulkGroupSelectionState(assets);
      if (!dirty) {
        draft = derived.groupKey;
        setGroupDraft(draft);
      }
      setOrder(derived.orderedAssetIds);
      setConflicting(derived.hasConflictingGroups);
    }
  );
  const singleId = createMemo(() => (selection.ids.size === 1 ? [...selection.ids][0] : undefined));
  createEffect(singleId, (id) => (id === undefined ? undefined : library.tags.pin(id)));
  createEffect(
    () => ({
      id: singleId(),
      enabled: selection.enabled,
      settings: settingsOpen(),
      retry: retry(),
      epoch: library.tags.epoch
    }),
    ({ id, enabled, settings }) => {
      let active = true;
      if (id === undefined || !enabled || settings) {
        setDetailsState("idle");
        return;
      }
      if (library.tags.get(id)) {
        setDetailsState("idle");
        return;
      }
      setDetailsState("loading");
      void readTags(id, () => active && !disposed);
      return () => {
        active = false;
      };
    }
  );
  /** Loads the complete tag base; a query refresh or write that discarded a read is retried. */
  async function readTags(id: number, active: () => boolean) {
    for (let attempt = 0; attempt < 3; attempt++) {
      const token = library.tags.captureGeneration(id);
      const generation = library.details.generation;
      try {
        await library.details.load(id);
      } catch {
        break;
      }
      if (!active()) return;
      if (library.tags.get(id)) {
        setDetailsState("idle");
        return;
      }
      // A current, unchanged read that published nothing is a genuine failure.
      if (library.tags.isCurrent(token) && generation === library.details.generation) break;
      await library.tags.waitForIdle();
      if (!active()) return;
    }
    if (active()) setDetailsState("failed");
  }
  onCleanup(() => {
    disposed = true;
  });
  function capture() {
    synchronize();
    const snapshot = selection.capture();
    const identity = library.tags.epoch;
    return {
      ...snapshot,
      current: () =>
        !disposed && snapshot.ids === selection.capture().ids && identity === library.tags.epoch,
      valid: () => !disposed && identity === library.tags.epoch
    };
  }
  async function saveTag(raw: string, remove = false): Promise<boolean> {
    const tag = remove ? raw : normalizeBulkTag(raw);
    const context = capture();
    const ids = [...context.ids];
    if (!context.enabled || context.busy || !ids.length || !tag || (remove && ids.length !== 1))
      return false;
    const base = ids.length === 1 ? library.tags.get(ids[0])?.tags : undefined;
    if (ids.length === 1 && !base) return false;
    // Applied to the canonical tags when the write leaves the asset's queue.
    const change = (tags: string[]) =>
      remove ? tags.filter((value) => value !== tag) : mergeTagLists(tags, [tag]);
    if (base && !remove && change(base).length === base.length) return true;
    tagWrites += 1;
    setTagApplying(true);
    setTagFailed(false);
    try {
      if (base) {
        await library.mutations.saveTags(ids[0], change);
      } else {
        const result = await library.mutations.mergeTags(ids, [tag]);
        if (context.current()) {
          setPartial({ processed: result.processed_assets, requested: ids.length });
          if (result.processed_assets) setAppliedTags((previous) => mergeTagLists(previous, [tag]));
          else setTagFailed(true);
        }
        if (!result.processed_assets) return false;
      }
      return context.valid();
    } catch {
      if (context.current()) setTagFailed(true);
      return false;
    } finally {
      tagWrites -= 1;
      if (!disposed && tagWrites === 0) setTagApplying(false);
    }
  }
  async function toggleFavorite() {
    const context = capture();
    if (!context.enabled || context.busy || !context.ids.size) return;
    favoriteWrites += 1;
    setFavoriteApplying(true);
    setFavoriteFailed(false);
    try {
      const result = await library.mutations.toggleFavorites([...context.ids]);
      if (!context.valid()) return;
      if (context.current())
        setPartial({ processed: result.processed_asset_ids.length, requested: context.ids.size });
      const processed = new Set(result.processed_asset_ids);
      const missing = new Set([...context.ids].filter((id) => !processed.has(id)));
      if (missing.size) selection.remove(missing);
    } catch {
      if (context.current()) setFavoriteFailed(true);
    } finally {
      favoriteWrites -= 1;
      if (!disposed && favoriteWrites === 0) setFavoriteApplying(false);
    }
  }
  async function applyGroup(explicitOrder?: number[]): Promise<BulkGroupSaveResult> {
    const context = capture();
    const ids = [...(explicitOrder ?? order())];
    if (
      !context.enabled ||
      context.busy ||
      !context.ids.size ||
      !metadata.ready ||
      ids.length !== context.ids.size ||
      new Set(ids).size !== ids.length ||
      ids.some((id) => !context.ids.has(id))
    )
      return { status: "ignored" };
    const key = normalizeGroupKey(draft) || null;
    const single = metadata.selected.length === 1 ? metadata.selected[0] : undefined;
    const preserves =
      single &&
      key &&
      normalizeGroupKey(single.media_group_key ?? "").toLowerCase() === key.toLowerCase();
    groupWrites += 1;
    setGroupApplying(true);
    setGroupFailed(false);
    try {
      const result = await library.mutations.setMediaGroups(
        ids.map((assetId, index) => ({
          assetId,
          mediaGroupOrder: key ? (preserves ? (single?.media_group_order ?? 1) : index + 1) : null
        })),
        key
      );
      if (!context.current()) return { status: "ignored" };
      setPartial({ processed: result.processed_assets, requested: ids.length });
      setOrder(ids);
      draft = key ?? "";
      setGroupDraft(draft);
      setConflicting(false);
      return result.processed_assets === ids.length
        ? { status: "saved" }
        : { status: "partial", processed: result.processed_assets, requested: ids.length };
    } catch {
      if (context.current()) setGroupFailed(true);
      return { status: "failed" };
    } finally {
      groupWrites -= 1;
      if (!disposed && groupWrites === 0) setGroupApplying(false);
    }
  }
  return {
    get selectedAssetIds() {
      return selection.ids;
    },
    get selectionBusy() {
      return selection.busy;
    },
    get selectedAssets() {
      return metadata.selected;
    },
    get metadataLoading() {
      return !metadata.ready && !metadata.failed;
    },
    get metadataFailed() {
      return metadata.failed;
    },
    onRetryMetadata: metadata.retry,
    get groupKeyDraft() {
      return groupDraft();
    },
    get orderedAssetIds() {
      return order();
    },
    get hasConflictingGroups() {
      return conflicting();
    },
    get groupApplying() {
      return groupApplying();
    },
    get groupFailed() {
      return groupFailed();
    },
    get favoriteApplying() {
      return favoriteApplying();
    },
    get favoriteFailed() {
      return favoriteFailed();
    },
    get allSelectedFavorites() {
      return (
        selection.ids.size > 0 &&
        metadata.ready &&
        metadata.selected.every((asset) => asset.is_favorite)
      );
    },
    get partialResult() {
      return partial();
    },
    get tagMode() {
      return selection.ids.size === 0 ? "none" : selection.ids.size === 1 ? "single" : "multiple";
    },
    get singleAssetTags() {
      const id = singleId();
      return id === undefined ? [] : (library.tags.get(id)?.tags ?? []);
    },
    get appliedBulkTags() {
      return appliedTags();
    },
    get tagDetailsLoading() {
      const id = singleId();
      return detailsState() === "loading" && (id === undefined || !library.tags.get(id));
    },
    get tagDetailsFailed() {
      const id = singleId();
      return detailsState() === "failed" && (id === undefined || !library.tags.get(id));
    },
    get tagApplying() {
      return tagApplying();
    },
    get tagSaveFailed() {
      return tagFailed();
    },
    get knownTags() {
      return library.vocabulary.knownTags;
    },
    onGroupKeyDraftChange(value: string) {
      synchronize();
      dirty = true;
      draft = value;
      setGroupDraft(value);
    },
    onApplyGroup: applyGroup,
    onToggleFavorite: toggleFavorite,
    onAddTag: (tag: string) => saveTag(tag),
    onRemoveTag: (tag: string) => saveTag(tag, true),
    onRetryTagDetails: () => setRetry((v) => v + 1),
    onBulkSelectionInteraction: selection.interact
  };
}
export type BulkController = ReturnType<typeof createBulk>;
