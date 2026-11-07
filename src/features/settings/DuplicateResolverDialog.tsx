import { createEffect, createMemo, createSignal, For, onSettled } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import { createVirtualList } from "../../ui/dom/createVirtualList";
import { UiButton } from "../../ui/controls/UiButton";
import { UiModal } from "../../ui/overlays/UiModal";
import { DuplicateGroupCard } from "./DuplicateGroupCard";
import { createDuplicateResolver } from "./createDuplicateResolver";
import { SectionOperationStatus } from "./SectionOperationStatus";
import type { DuplicateGroup } from "../../platform/tauri/contracts/maintenance";
import type {
  DuplicateResolutionChange,
  SectionOperationState
} from "./types";
import type { ThumbnailStore } from "../../library/thumbnails/thumbnailStore";

interface Props {
  open: boolean;
  isOperationLocked: boolean;
  operationState: SectionOperationState;
  groups: DuplicateGroup[];
  thumbnailStore: ThumbnailStore;
  onQueueThumbnailsByIds: (ids: number[]) => void;
  onClose: () => void;
  onRescan: () => void;
  onSaveAll: (changes: DuplicateResolutionChange[]) => Promise<void>;
}
function assetId(element: HTMLElement): number | null {
  const id = Number(element.dataset.assetId);
  return Number.isInteger(id) && id > 0 ? id : null;
}
export function DuplicateResolverDialog(props: Props) {
  const { t } = useLanguage();
  const state = createDuplicateResolver(props);
  const [scroller, setScroller] = createSignal<HTMLDivElement>();
  const list = createVirtualList({
    scroller,
    count: () => props.groups.length,
    estimateSize: () => 360,
    overscan: 2,
    initialViewport: 720,
    enabled: () => props.open
  });
  const snapshot = createMemo(() => {
    const items = list.items();
    return {
      items,
      starts: new Map(items.map((item) => [item.index, item.start])),
      height: list.totalSize()
    };
  });
  // Keep row identities for the scan snapshot; virtualizer updates only change geometry.
  const rows = createMemo(() => props.groups.map((group, index) => ({ group, index })));
  const renderedGroups = createMemo(() => {
    const groups = rows();
    const items = snapshot().items;
    if (!items.length) return groups.slice(0, 3);
    // Virtual measurements can still describe the previous scan during this update.
    // Carry each existing group with its row so children never index a newer snapshot.
    return items.flatMap((item) => {
      const group = groups[item.index];
      return group ? [group] : [];
    });
  });
  createEffect(
    () =>
      props.open
        ? [
            ...new Set(
              snapshot().items.flatMap((item) =>
                (props.groups[item.index]?.assets ?? []).map((asset) => asset.id)
              )
            )
          ]
        : [],
    (ids) => {
      if (ids.length) props.onQueueThumbnailsByIds(ids);
    }
  );
  const hasChanges = () => state.stagedChanges.length > 0;
  const canSave = () => !props.isOperationLocked && hasChanges() && state.validationResult.valid;
  return (
    <UiModal
      open={props.open}
      onClose={props.onClose}
      closeOnOverlayClick={!props.isOperationLocked}
      closeOnEscape={!props.isOperationLocked}
      size="large"
      labelledBy="duplicate-resolver-heading"
      contentClassName="grid h-[min(88vh,900px)] grid-rows-[auto_auto_auto_1fr] gap-3"
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="grid gap-1">
          <h3 id="duplicate-resolver-heading" class="m-0">
            {t("settings.duplicates.resolverHeading")}
          </h3>
          <p class="m-0 text-xs text-[var(--text-muted)]">
            {t("settings.duplicates.resolverSummary", {
              groups: props.groups.length,
              assets: state.duplicateAssetCount
            })}
          </p>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <UiButton
            onClick={() => {
              if (canSave()) void state.save();
            }}
            disabled={!canSave()}
          >
            {t("settings.duplicates.saveAll")}
          </UiButton>
          <UiButton onClick={state.resetForm} disabled={props.isOperationLocked}>
            {t("settings.duplicates.discardChanges")}
          </UiButton>
          <UiButton onClick={props.onRescan} disabled={props.isOperationLocked}>
            {t("settings.scan.rescan")}
          </UiButton>
          <UiButton onClick={props.onClose} disabled={props.isOperationLocked}>
            {t("common.close")}
          </UiButton>
        </div>
      </div>
      <SectionOperationStatus state={props.operationState} loaderTestId="duplicates-modal-loader" />
      <div class="text-xs text-[var(--text-muted)]">
        {!hasChanges()
          ? t("settings.duplicates.queueHint")
          : state.validationResult.valid
            ? t("settings.duplicates.queuedReady", { count: state.stagedChanges.length })
            : state.validationResult.firstError}
      </div>
      <div ref={setScroller} class="overflow-auto pr-1">
        {!props.groups.length && !props.operationState.loading ? (
          <div class="rounded-[var(--radius-control)] border border-base-content/20 bg-base-200/40 p-3 text-sm text-[var(--text-muted)]">
            {t("settings.duplicates.noGroups")}
          </div>
        ) : null}
        <div class="relative" style={{ height: `${snapshot().height}px` }}>
          <For each={renderedGroups()}>
            {(item) => (
              <div
                ref={(element) =>
                  onSettled(() => {
                    if (element.isConnected) list.measure(element, item.index);
                  })
                }
                data-index={item.index}
                class="absolute left-0 top-0 w-full pb-2"
                style={{
                  transform: `translateY(${snapshot().starts.get(item.index) ?? item.index * 360}px)`
                }}
              >
                <DuplicateGroupCard
                  group={item.group}
                  renameValues={state.renameValues}
                  stagedChangesByAssetId={state.stagedChangesByAssetId}
                  thumbnailStore={props.thumbnailStore}
                  isOperationLocked={props.isOperationLocked}
                  t={t}
                  onRenameValueChange={(event) => {
                    const id = assetId(event.currentTarget);
                    if (id) state.rename(id, event.currentTarget.value);
                  }}
                  onGenerateUuidClick={(event) => {
                    const id = assetId(event.currentTarget);
                    const name = event.currentTarget.dataset.currentName;
                    if (id && name) state.generateUuid(id, name);
                  }}
                  onQueueRenameClick={(event) => {
                    const id = assetId(event.currentTarget);
                    if (id) state.queueRename(id);
                  }}
                  onToggleDeleteClick={(event) => {
                    const id = assetId(event.currentTarget);
                    if (id) state.toggleDelete(id);
                  }}
                  onClearActionClick={(event) => {
                    const id = assetId(event.currentTarget);
                    if (id) state.clear(id);
                  }}
                />
              </div>
            )}
          </For>
        </div>
      </div>
    </UiModal>
  );
}
