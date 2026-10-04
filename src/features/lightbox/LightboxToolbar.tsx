import { UiIconButton } from "../../ui/controls/UiIconButton";
import { LightboxInfoPanel } from "./LightboxInfoPanel";
import { LightboxTagPanel } from "./LightboxTagPanel";
import { LightboxDeleteConfirmDialog } from "./LightboxDeleteConfirmDialog";
import { MediaGroupSetter } from "./MediaGroupSetter";
import type { SelectedAsset } from "./types";
import type { SuggestionClient } from "../search/suggestions/SuggestionClient";

export const SIDEBAR_CLOSE_BUTTON_ID = "lightbox-sidebar-close-button";

interface LightboxToolbarProps {
  selected: SelectedAsset;
  mediaGroupKeyEditor: string;
  mediaGroupOrderEditor: string;
  isNarrow: boolean;
  sidebarOpen: boolean;
  sidebarVisible: boolean;
  selectedTags: string[];
  tagDraft: string;
  knownTags: string[];
  tagInputRef: (node: HTMLInputElement) => void;
  client: SuggestionClient;
  tagSaving: boolean;
  tagFailed: boolean;
  tagDetailsLoading: boolean;
  tagDetailsFailed: boolean;
  tagEditingDisabled: boolean;
  onRemoveTag: (tag: string) => void;
  onTagDraftChange: (value: string) => void;
  onAddTag: (value: string) => void;
  onRetryTags: () => void;
  onRetryTagDetails: () => void;
  onMediaGroupKeyChange: (value: string) => void;
  onMediaGroupOrderChange: (value: string) => void;
  onApplyMediaGroup: () => void;
  infoPanelOpen: boolean;
  onToggleInfo: () => void;
  onCloseSidebar: (restoreFocus: boolean) => void;
  onCloseLightbox: () => void;
  groupPending?: boolean;
  groupCopyConfirmed: boolean;
  canCopyMediaGroup: boolean;
  copyMediaGroupTitle: string;
  onCopyMediaGroup: () => void;
  deleteConfirmOpen: boolean;
  deleteSubmitting: boolean;
  deleteError?: string | null;
  onDeleteConfirmClose: () => void;
  onDeleteConfirmSubmit: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

export function LightboxToolbar(props: LightboxToolbarProps) {
  return (
    <aside
      inert={!props.sidebarVisible}
      id="lightbox-sidebar"
      class={[
        "grid h-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)_auto] gap-3 overflow-hidden border-l border-[var(--border-soft)] bg-[var(--surface-solid)] p-4",
        "absolute inset-y-0 right-0 z-[8] transition-transform duration-200 motion-reduce:transition-none",
        props.isNarrow ? "w-[min(340px,100%)]" : "w-[340px]",
        props.sidebarVisible ? "translate-x-0" : "pointer-events-none translate-x-full"
      ].join(" ")}
      data-lightbox-toolbar={props.selected.kind}
      aria-label={props.t("lightbox.sidebarAria")}
      aria-hidden={!props.sidebarVisible ? "true" : "false"}
    >
      <header class="flex min-h-0 min-w-0 items-center justify-between gap-2 border-b border-[var(--border-soft)] pb-3">
        <span
          class="min-w-0 truncate text-sm font-semibold text-base-content"
          title={props.selected.file_name}
        >
          {props.selected.file_name}
        </span>
        <div class="flex shrink-0 items-center gap-1">
          <UiIconButton
            id={SIDEBAR_CLOSE_BUTTON_ID}
            icon="arrow-left"
            iconClassName="h-3.5 w-3.5 shrink-0 rotate-180"
            className="h-8! w-8! min-h-8!"
            aria-label={props.t("lightbox.closePanel")}
            aria-expanded={props.sidebarOpen ? "true" : "false"}
            aria-controls="lightbox-sidebar"
            title={props.t("lightbox.closePanel")}
            disabled={props.deleteConfirmOpen || props.deleteSubmitting}
            onClick={(event) => props.onCloseSidebar(event.detail === 0)}
          />
          <UiIconButton
            icon="close"
            className="h-8! w-8! min-h-8!"
            aria-label={props.t("lightbox.closePreview")}
            title={props.t("common.close")}
            disabled={props.deleteSubmitting}
            onClick={props.onCloseLightbox}
          />
        </div>
      </header>

      <div
        class="panel-scroll flex min-h-0 min-w-0 flex-col gap-4 overflow-x-hidden overflow-y-auto pr-2"
        data-testid="lightbox-sidebar-upper"
      >
        <section
          aria-label={props.t("lightbox.mediaGroup")}
          data-testid="lightbox-media-group-panel"
          class="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5"
        >
          <h3 class="m-0 text-xs">{props.t("lightbox.mediaGroup")}</h3>
          <MediaGroupSetter
            groupCopyConfirmed={props.groupCopyConfirmed}
            canCopyMediaGroup={props.canCopyMediaGroup}
            copyMediaGroupTitle={props.copyMediaGroupTitle}
            onCopyMediaGroup={props.onCopyMediaGroup}
            groupKey={props.mediaGroupKeyEditor}
            groupOrder={props.mediaGroupOrderEditor}
            onGroupKeyChange={props.onMediaGroupKeyChange}
            onGroupOrderChange={props.onMediaGroupOrderChange}
            onApply={props.onApplyMediaGroup}
            pending={props.groupPending}
          />
        </section>

        <LightboxTagPanel
          selectedTags={props.selectedTags}
          tagDraft={props.tagDraft}
          client={props.client}
          knownTags={props.knownTags}
          tagInputRef={props.tagInputRef}
          tagSaving={props.tagSaving}
          tagFailed={props.tagFailed}
          tagDetailsLoading={props.tagDetailsLoading}
          tagDetailsFailed={props.tagDetailsFailed}
          tagEditingDisabled={props.tagEditingDisabled}
          onRemoveTag={props.onRemoveTag}
          onTagDraftChange={props.onTagDraftChange}
          onAddTag={props.onAddTag}
          onRetryTags={props.onRetryTags}
          onRetryTagDetails={props.onRetryTagDetails}
        />
        <button
          type="button"
          class="flex items-center justify-between border-t border-[var(--border-soft)] py-3 text-xs text-[var(--text-muted)]"
          aria-expanded={props.infoPanelOpen ? "true" : "false"}
          onClick={props.onToggleInfo}
        >
          {props.t("lightbox.infoHeading")}{" "}
          <span aria-hidden="true">{props.infoPanelOpen ? "−" : "+"}</span>
        </button>
        {props.infoPanelOpen ? <LightboxInfoPanel selected={props.selected} /> : null}
      </div>

      <div
        class={`grid max-h-[calc(100dvh-100px)] min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5 ${props.deleteConfirmOpen ? "panel-scroll overflow-y-auto" : "overflow-visible"}`}
      >
        <LightboxDeleteConfirmDialog
          open={props.deleteConfirmOpen}
          focusReady={props.sidebarVisible}
          restoreFocusId={props.isNarrow ? SIDEBAR_CLOSE_BUTTON_ID : "lightbox-delete-button"}
          isSubmitting={props.deleteSubmitting}
          errorMessage={props.deleteError}
          onClose={props.onDeleteConfirmClose}
          onConfirm={props.onDeleteConfirmSubmit}
        />
      </div>
    </aside>
  );
}
