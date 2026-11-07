import { For } from "solid-js";
type ChangeEvent<T> = Event & {
  currentTarget: T;
};
type ButtonEvent<T> = MouseEvent & {
  currentTarget: T;
};
import { isDuplicateGroupResolved } from "./duplicateValidationService";
import type { DuplicateAsset, DuplicateGroup } from "../../platform/tauri/contracts/maintenance";
import type { DuplicateResolutionChange } from "./types";
import { DuplicateAssetCard } from "./DuplicateAssetCard";
interface DuplicateGroupCardProps {
  group: DuplicateGroup;
  renameValues: Record<number, string>;
  stagedChangesByAssetId: Record<number, DuplicateResolutionChange>;
  isOperationLocked: boolean;
  t: (key: string, options?: Record<string, unknown>) => string;
  onRenameValueChange: (event: ChangeEvent<HTMLInputElement>) => void;
  onGenerateUuidClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onQueueRenameClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onToggleDeleteClick: (event: ButtonEvent<HTMLButtonElement>) => void;
  onClearActionClick: (event: ButtonEvent<HTMLButtonElement>) => void;
}
export function DuplicateGroupCard(props: DuplicateGroupCardProps) {
  const isResolved = () => isDuplicateGroupResolved(props.group, props.stagedChangesByAssetId);
  return (
    <section
      class={`grid gap-2 rounded-[var(--radius-control)] border p-3 ${isResolved() ? "border-primary/40 bg-primary/10" : "border-base-content/20 bg-base-100/75"}`}
    >
      <div class="flex items-center justify-between gap-2">
        <h4 class="m-0 text-sm">{props.group.file_name}</h4>
        {isResolved() ? (
          <span class="rounded-[var(--radius-control)] border border-primary/40 bg-primary/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-primary-text">
            {props.t("settings.duplicates.readyBadge")}
          </span>
        ) : null}
      </div>

      <For each={props.group.assets}>
        {(asset) => (
          <DuplicateAssetCard
            asset={asset}
            renameValue={props.renameValues[asset.id] ?? getCurrentName(asset)}
            staged={props.stagedChangesByAssetId[asset.id] ?? null}
            isOperationLocked={props.isOperationLocked}
            t={props.t}
            onRenameValueChange={props.onRenameValueChange}
            onGenerateUuidClick={props.onGenerateUuidClick}
            onQueueRenameClick={props.onQueueRenameClick}
            onToggleDeleteClick={props.onToggleDeleteClick}
            onClearActionClick={props.onClearActionClick}
          />
        )}
      </For>
    </section>
  );
}
function getCurrentName(asset: DuplicateAsset) {
  return asset.path.split(/[\\/]/).pop() ?? asset.path;
}
