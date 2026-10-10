import { createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import type { createLibrary } from "../../library/createLibrary";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import type { SelectedAsset } from "./types";
import { normalizeTags } from "../../utils/media";
import { parseMediaGroupOrder } from "./parseMediaGroupOrder";

type Navigation = "ready" | "resolving" | "missing" | "failed";
type WriteState = "idle" | "pending" | "failed";
interface TagIntent {
  confirmed: string[];
  desired: string[] | null;
  state: WriteState;
}
const sameTags = (a: string[], b: string[]) =>
  a.length === b.length && a.every((v, i) => v === b[i]);
function rebase(intent: TagIntent, canonical: string[]) {
  if (intent.desired) {
    const removed = new Set(intent.confirmed.filter((tag) => !intent.desired?.includes(tag)));
    intent.desired = normalizeTags([
      ...canonical.filter((tag) => !removed.has(tag)),
      ...intent.desired.filter((tag) => !intent.confirmed.includes(tag))
    ]);
  }
  intent.confirmed = canonical;
}

/** Application-owned selection. Commands advance their targets before Solid publishes state. */
export function createLightbox(library: ReturnType<typeof createLibrary>) {
  const { query, tags, details, mutations } = library;
  let current: SelectedAsset | null = null;
  const [selected, publishSelected] = createSignal<SelectedAsset | null>(null);
  const [detailState, setDetailState] = createSignal<"loading" | "ready" | "failed">("ready");
  const [navigation, setNavigation] = createSignal<Navigation>("ready");
  const [editor, setEditor] = createSignal<string[]>([]);
  const [tagState, setTagState] = createSignal<WriteState>("idle");
  const [favoriteState, setFavoriteState] = createSignal<WriteState>("idle");
  const [groupState, setGroupState] = createSignal<WriteState>("idle");
  const [groupKey, publishGroupKey] = createSignal("");
  const [groupOrder, publishGroupOrder] = createSignal("");
  let keyDraft = "",
    orderDraft = "";
  let keyDirty = false,
    orderDirty = false;
  let selectionRequest = 0,
    navigationRequest = 0,
    positionRequest = 0;
  let selectedIndex: number | null = null,
    targetIndex: number | null = null;
  let queryEpoch = query.navigationSnapshot().epoch;
  let disposed = false;
  let releasePin = () => {};
  const intents = new Map<number, TagIntent>();
  const favorites = new Set<number>(),
    groups = new Set<number>();

  function publish(value: SelectedAsset | null) {
    const changed = current?.id !== value?.id;
    if (changed && current) {
      const intent = intents.get(current.id);
      if (intent?.state === "idle" && !intent.desired) intents.delete(current.id);
    }
    current = value;
    if (changed) {
      releasePin();
      releasePin = value ? tags.pin(value.id) : () => {};
      keyDirty = orderDirty = false;
      setFavoriteState(value && favorites.has(value.id) ? "pending" : "idle");
      setGroupState(value && groups.has(value.id) ? "pending" : "idle");
    }
    if (!keyDirty) {
      keyDraft = value?.media_group_key ?? "";
      publishGroupKey(keyDraft);
    }
    if (!orderDirty) {
      orderDraft = value?.media_group_order?.toString() ?? "";
      publishGroupOrder(orderDraft);
    }
    publishSelected(value);
    syncTags();
  }
  function syncTags() {
    const intent = current ? intents.get(current.id) : undefined;
    setEditor(intent?.desired ?? (current ? (tags.get(current.id)?.tags ?? current.tags) : []));
    setTagState(intent?.state ?? "idle");
  }
  function valid(request: number, epoch: number) {
    return (
      !disposed &&
      request === selectionRequest &&
      current !== null &&
      epoch === tags.captureGeneration(current.id).epoch
    );
  }
  function prefetch(index: number) {
    const count = query.navigationSnapshot().total,
      epoch = query.navigationSnapshot().epoch;
    if (count <= 1) return;
    for (const adjacent of new Set([(index - 1 + count) % count, (index + 1) % count])) {
      void query
        .getAssetAtAsync(adjacent)
        .then((asset) => {
          if (!disposed && query.navigationSnapshot().epoch === epoch && asset)
            return details.load(asset.id);
        })
        .catch(() => {});
    }
  }
  async function readDetails(id: number) {
    const request = ++selectionRequest;
    const epoch = tags.captureGeneration(id).epoch;
    setDetailState("loading");
    try {
      // A metadata write invalidates the entire record, not just its tag list.
      while (valid(request, epoch) && current?.id === id) {
        const token = tags.captureGeneration(id),
          queryVersion = query.navigationSnapshot().epoch;
        const value = await details.load(id);
        if (!valid(request, epoch) || current?.id !== id) return;
        if (!tags.isCurrent(token) || queryVersion !== query.navigationSnapshot().epoch) {
          await tags.waitForIdle();
          continue;
        }
        if (!value) {
          setDetailState("failed");
          return;
        }
        publish(value);
        setDetailState("ready");
        if (selectedIndex !== null) prefetch(selectedIndex);
        return;
      }
    } catch {
      if (valid(request, epoch)) setDetailState("failed");
    }
  }
  function resolvePosition(changedQuery = false) {
    const request = ++positionRequest;
    navigationRequest++;
    if (!current) return;
    const id = current.id;
    const index = query.navigationSnapshot().pending
      ? null
      : changedQuery
        ? query.getAssetIndex(id)
        : (selectedIndex ?? query.getAssetIndex(id));
    selectedIndex = targetIndex = index;
    if (index !== null) {
      setNavigation("ready");
      return;
    }
    setNavigation("resolving");
    if (query.navigationSnapshot().pending) return;
    const epoch = query.navigationSnapshot().epoch;
    void query
      .getAssetPosition(id)
      .then((result) => {
        if (
          disposed ||
          request !== positionRequest ||
          current?.id !== id ||
          query.navigationSnapshot().epoch !== epoch
        )
          return;
        if (result.status === "resolved") {
          selectedIndex = targetIndex = result.index;
          setNavigation("ready");
        } else setNavigation(result.status === "missing" ? "missing" : "failed");
      })
      .catch(() => {
        if (!disposed && request === positionRequest) setNavigation("failed");
      });
  }
  function select(summary: AssetSummary | null, index?: number) {
    selectionRequest++;
    navigationRequest++;
    positionRequest++;
    selectedIndex = targetIndex = summary ? (index ?? query.getAssetIndex(summary.id)) : null;
    if (!summary) {
      publish(null);
      setDetailState("ready");
      return;
    }
    const cached = details.get(summary.id);
    publish(
      cached ?? {
        ...summary,
        path: summary.preview_path,
        size_bytes: null,
        tags: tags.get(summary.id)?.tags ?? []
      }
    );
    resolvePosition();
    if (cached) {
      setDetailState("ready");
      if (selectedIndex !== null) prefetch(selectedIndex);
    } else void readDetails(summary.id);
  }
  function navigate(step: -1 | 1) {
    // Refresh commands change the query immediately, before reactive effects run.
    if (query.navigationSnapshot().epoch !== queryEpoch || query.navigationSnapshot().pending)
      return;
    const previous = targetIndex ?? selectedIndex;
    if (previous === null || query.navigationSnapshot().total === 0 || !current) return;
    const next =
      (previous + step + query.navigationSnapshot().total) % query.navigationSnapshot().total;
    targetIndex = next;
    const request = ++navigationRequest,
      epoch = query.navigationSnapshot().epoch;
    void query
      .getAssetAtAsync(next)
      .then((asset) => {
        if (disposed || request !== navigationRequest || epoch !== query.navigationSnapshot().epoch)
          return;
        if (asset) select(asset, next);
        else targetIndex = previous;
      })
      .catch(() => {
        if (request === navigationRequest) targetIndex = previous;
      });
  }
  function drain(id: number) {
    const intent = intents.get(id);
    if (!intent?.desired || intent.state !== "idle" || !tags.get(id)) return;
    const requested = intent.desired;
    intent.state = "pending";
    syncTags();
    void mutations
      .saveTags(id, requested)
      .then((result) => {
        if (disposed || intents.get(id) !== intent) return;
        intent.confirmed = result.tags;
        if (intent.desired && sameTags(intent.desired, requested)) intent.desired = null;
        intent.state = "idle";
      })
      .catch(() => {
        if (intents.get(id) === intent) intent.state = "failed";
      })
      .finally(() => {
        if (disposed || intents.get(id) !== intent) return;
        syncTags();
        if (intent.state === "idle" && intent.desired) drain(id);
        else if (intent.state === "idle" && current?.id !== id) intents.delete(id);
      });
  }
  function saveTags(next: string[]) {
    if (!current) return;
    const canonical = tags.get(current.id);
    if (!canonical) return;
    const intent = intents.get(current.id) ?? {
      confirmed: canonical.tags,
      desired: null,
      state: "idle"
    };
    intent.desired = normalizeTags(next);
    if (intent.state !== "pending") intent.state = "idle";
    intents.set(current.id, intent);
    syncTags();
    drain(current.id);
  }
  createEffect(
    () => tags.epoch,
    (epoch, previous) => {
      if (previous !== undefined && epoch !== previous) {
        intents.clear();
        untrack(() => select(null));
      }
    }
  );
  createEffect(
    () => ({ epoch: query.queryEpoch, pending: query.queryPending }),
    ({ epoch }) => {
      const changed = epoch !== queryEpoch;
      queryEpoch = epoch;
      untrack(() => resolvePosition(changed));
    }
  );
  createEffect(
    () => tags.revision,
    () =>
      untrack(() => {
        if (!current) return;
        const canonical = tags.get(current.id);
        if (!canonical) return;
        const intent = intents.get(current.id);
        if (intent && intent.state !== "pending") rebase(intent, canonical.tags);
        if (!sameTags(current.tags, canonical.tags)) publish({ ...current, tags: canonical.tags });
        else syncTags();
      })
  );
  const selectedId = createMemo(() => selected()?.id);
  createEffect(
    () => {
      const id = selectedId();
      return query.assets.find((asset) => asset.id === id);
    },
    (summary) => {
      if (!summary || !current || summary.id !== current.id) return;
      const previous = current;
      const next = {
        ...summary,
        path: current.path ?? summary.preview_path,
        size_bytes: current.size_bytes,
        tags: current.tags
      };
      if (
        Object.keys(summary).some((key) => Reflect.get(previous, key) !== Reflect.get(summary, key))
      )
        untrack(() => publish(next));
    }
  );
  onCleanup(
    mutations.subscribe((commit) => {
      if (!current) return;
      if (commit.type === "deleted") {
        if (commit.ids.includes(current.id)) select(null);
      } else {
        const patch = commit.patches.get(current.id);
        if (patch) publish({ ...current, ...patch });
      }
    })
  );
  onCleanup(() => {
    disposed = true;
    selectionRequest++;
    navigationRequest++;
    positionRequest++;
    releasePin();
    intents.clear();
  });
  return {
    get selected() {
      return selected();
    },
    get navigationStatus() {
      return navigation();
    },
    get assetDetailsFailed() {
      return detailState() === "failed";
    },
    get tagDetailsLoading() {
      const asset = selected();
      return !!asset && !tags.get(asset.id) && detailState() === "loading";
    },
    get tagDetailsFailed() {
      const asset = selected();
      return !!asset && !tags.get(asset.id) && detailState() === "failed";
    },
    get tagEditingDisabled() {
      const asset = selected();
      return !asset || !tags.get(asset.id);
    },
    get tagEditor() {
      return editor();
    },
    get tagSaving() {
      return tagState() === "pending";
    },
    get tagFailed() {
      return tagState() === "failed";
    },
    get favoritePending() {
      return favoriteState() === "pending";
    },
    get favoriteFailed() {
      return favoriteState() === "failed";
    },
    get groupPending() {
      return groupState() === "pending";
    },
    get groupFailed() {
      return groupState() === "failed";
    },
    get mediaGroupKeyEditor() {
      return groupKey();
    },
    get mediaGroupOrderEditor() {
      return groupOrder();
    },
    setMediaGroupKeyEditor(value: string) {
      keyDirty = true;
      keyDraft = value;
      publishGroupKey(value);
    },
    setMediaGroupOrderEditor(value: string) {
      if (parseMediaGroupOrder(value) !== undefined) {
        orderDirty = true;
        orderDraft = value;
        publishGroupOrder(value);
      }
    },
    select,
    close: () => select(null),
    saveTags,
    previous: () => navigate(-1),
    next: () => navigate(1),
    retryDetails: () => {
      if (current) void readDetails(current.id);
    },
    retryTags() {
      if (!current) return;
      const intent = intents.get(current.id),
        canonical = tags.get(current.id);
      if (!intent || !canonical || intent.state === "pending") return;
      rebase(intent, canonical.tags);
      intent.state = "idle";
      drain(current.id);
    },
    async retryNavigation() {
      const request = selectionRequest;
      try {
        await query.refresh();
        if (selectionRequest === request) resolvePosition(true);
      } catch {
        if (selectionRequest === request) setNavigation("failed");
      }
    },
    async toggleFavorite() {
      const asset = current;
      if (!asset || favorites.has(asset.id)) return;
      favorites.add(asset.id);
      setFavoriteState("pending");
      const epoch = tags.captureGeneration(asset.id).epoch;
      try {
        await mutations.setFavorite(asset.id, !asset.is_favorite);
        if (
          !disposed &&
          current?.id === asset.id &&
          epoch === tags.captureGeneration(asset.id).epoch
        )
          setFavoriteState("idle");
      } catch {
        if (
          !disposed &&
          current?.id === asset.id &&
          epoch === tags.captureGeneration(asset.id).epoch
        )
          setFavoriteState("failed");
      } finally {
        favorites.delete(asset.id);
      }
    },
    async saveMediaGroup() {
      const asset = current,
        order = parseMediaGroupOrder(orderDraft),
        key = keyDraft;
      if (!asset || order === undefined || groups.has(asset.id)) return;
      groups.add(asset.id);
      setGroupState("pending");
      const epoch = tags.captureGeneration(asset.id).epoch;
      try {
        await mutations.setMediaGroup(asset.id, key.trim() || null, order);
        if (
          !disposed &&
          current?.id === asset.id &&
          epoch === tags.captureGeneration(asset.id).epoch
        ) {
          if (keyDraft === key) keyDirty = false;
          if (parseMediaGroupOrder(orderDraft) === order) orderDirty = false;
          publish(current);
          setGroupState("idle");
        }
      } catch {
        if (
          !disposed &&
          current?.id === asset.id &&
          epoch === tags.captureGeneration(asset.id).epoch
        )
          setGroupState("failed");
      } finally {
        groups.delete(asset.id);
      }
    },
    deleteSelected: () => (current ? mutations.deleteAsset(current.id) : Promise.resolve(null))
  };
}
export type LightboxModel = ReturnType<typeof createLightbox>;
