import { createEffect, createMemo, createSignal, onCleanup, Show } from "solid-js";
import { Portal } from "@solidjs/web";
import { useLanguage } from "../../i18n/solid";
import type { SuggestionClient } from "../search/suggestions/SuggestionClient";
import { createModalPresence, createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { createUiLayer } from "../../ui/overlays/UiLayerProvider";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiIconButton } from "../../ui/controls/UiIconButton";
import { isValidTag, normalizeTags } from "../../utils/media";
import { LightboxToolbar, SIDEBAR_CLOSE_BUTTON_ID } from "./LightboxToolbar";
import { LightboxActionRail } from "./LightboxActionRail";
import { LightboxMediaStage } from "./LightboxMediaStage";
import { createImageControls } from "./image/createImageControls";
import {
  createLightboxActivity,
  createLightboxSidebar,
  createGroupClipboard
} from "./createLightboxPresentation";
import type { LightboxModel } from "./createLightbox";
import type { VideoBounds } from "../../platform/tauri/contracts/video";
import type { SelectedAsset } from "./types";

const TRIGGER_ID = "lightbox-sidebar-trigger-button";
interface Props {
  model: LightboxModel;
  client: SuggestionClient;
  knownTags: string[];
  getRestoreFocus?: () => HTMLElement | null;
}
export default function LightboxModal(props: Props) {
  const active = createMemo(() => props.model.selected !== null);
  const presence = createModalPresence(active);
  const snapshot = createModalSnapshot(active, () => ({ ...props.model }));
  const session = createMemo<number>((previous) =>
    active() ? (previous ?? 0) + 1 : (previous ?? 0)
  );
  return (
    <Show when={presence.present()}>
      <Show when={session()} keyed>
        {(_session) => (
          <Show when={snapshot().selected}>
            {(selected) => (
              <LightboxContent
                {...props}
                model={snapshot()}
                active={active()}
                presence={presence}
                selected={selected()}
                tags={snapshot().tagEditor}
                groupKey={snapshot().mediaGroupKeyEditor}
                groupOrder={snapshot().mediaGroupOrderEditor}
              />
            )}
          </Show>
        )}
      </Show>
    </Show>
  );
}
function LightboxContent(
  props: Props & {
    active: boolean;
    presence: ReturnType<typeof createModalPresence>;
    selected: SelectedAsset;
    tags: string[];
    groupKey: string;
    groupOrder: string;
  }
) {
  const { t } = useLanguage();
  const [info, setInfo] = createSignal(false),
    [confirm, setConfirm] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false),
    [deleteError, setDeleteError] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal("");
  let deletingNow = false,
    generation = 0,
    input: HTMLInputElement | undefined;
  let disposed = false,
    focusFrame: number | undefined;
  createEffect(
    () => props.selected.id,
    () => {
      generation++;
      setInfo(false);
      setConfirm(false);
      setDeleteError(null);
      setDraft("");
      deletingNow = false;
      setDeleting(false);
    }
  );
  onCleanup(() => {
    disposed = true;
    generation++;
    if (focusFrame !== undefined) cancelAnimationFrame(focusFrame);
  });
  const close = () => {
    if (!deletingNow) props.model.close();
  };
  const sidebar = createLightboxSidebar(
    () => true,
    () => confirm() || deleting()
  );
  const [nativePanelReady, setNativePanelReady] = createSignal(false);
  const openSidebar = () => {
    if (!sidebar.occupied()) setNativePanelReady(false);
    sidebar.show();
  };
  const sidebarVisible = () =>
    sidebar.open() && (props.selected.kind !== "video" || nativePanelReady());
  const videoFullscreen = () => props.selected.kind === "video" && controls.isFullscreen;
  const handleNativeBounds = (bounds: VideoBounds) => {
    const viewport = controls.viewport()?.getBoundingClientRect();
    if (viewport && (bounds.width === 0 || bounds.x + bounds.width <= viewport.right + 1))
      setNativePanelReady(true);
  };
  const activity = createLightboxActivity(() => props.active);
  const controls = createImageControls({
    selected: () => props.selected,
    active: () => props.active,
    keyboardEnabled: () =>
      layer.isTopLayer() &&
      !confirm() &&
      !deleting() &&
      (!(sidebar.narrow() && sidebar.open()) || videoFullscreen()),
    close,
    previous: props.model.previous,
    next: props.model.next,
    enterFullscreen: () => setInfo(false)
  });
  const clipboard = createGroupClipboard(
    () => props.groupKey,
    () => props.selected.id
  );
  const layer = createUiLayer({
    active: () => props.active,
    modal: true,
    container: controls.shell,
    closeOnEscape: () => !controls.isFullscreen && !confirm() && !deleting(),
    onEscape: dismissWindowedLayer,
    getRestoreFocus: props.getRestoreFocus
  });
  function dismissWindowedLayer() {
    if (sidebar.narrow() && sidebar.open()) closeSidebar();
    else close();
  }
  function handleNativeEscape() {
    if (!props.active || !layer.isTopLayer() || controls.isFullscreen || deletingNow) return;
    if (confirm()) setConfirm(false);
    else dismissWindowedLayer();
  }
  let restoreTriggerFocus = true,
    wasOpen: boolean | undefined;
  function closeSidebar(restoreFocus = true) {
    restoreTriggerFocus = restoreFocus;
    sidebar.hide();
  }
  createEffect(
    () => ({
      open: sidebar.open(),
      visible: sidebarVisible(),
      occupied: sidebar.occupied(),
      narrow: sidebar.narrow(),
      kind: props.selected.kind,
      active: props.active,
      confirm: confirm()
    }),
    ({ open, visible, occupied, narrow, kind, active, confirm }) => {
      if (!active) return;
      if (wasOpen && !open && kind === "video" && narrow && occupied) return;
      if (wasOpen === undefined || wasOpen === visible) {
        wasOpen = visible;
        return;
      }
      const frame = requestAnimationFrame(() => {
        const previouslyOpen = wasOpen;
        wasOpen = visible;
        if (visible) {
          document
            .getElementById(confirm ? "lightbox-delete-confirm-input" : SIDEBAR_CLOSE_BUTTON_ID)
            ?.focus({ preventScroll: true });
        } else if (previouslyOpen && !open) {
          const target = restoreTriggerFocus
            ? document.getElementById(TRIGGER_ID)
            : controls.shell();
          target?.focus({ preventScroll: true });
          activity.reveal();
        }
      });
      return () => cancelAnimationFrame(frame);
    }
  );
  function addTag(value: string) {
    if (props.model.tagEditingDisabled) return;
    const tag = value.trim().toLowerCase();
    if (!isValidTag(tag) || props.model.tagEditor.includes(tag)) return;
    props.model.saveTags(normalizeTags([...props.model.tagEditor, tag]));
    setDraft("");
    const request = generation;
    if (focusFrame !== undefined) cancelAnimationFrame(focusFrame);
    focusFrame = requestAnimationFrame(() => {
      if (!disposed && props.active && generation === request) input?.focus();
    });
  }
  async function deleteMedia() {
    if (deletingNow) return;
    const request = generation;
    deletingNow = true;
    setDeleting(true);
    setDeleteError(null);
    try {
      const result = await props.model.deleteSelected();
      if (result?.source_status === "missing")
        window.alert(t("lightbox.deleteConfirm.sourceMissing"));
      else if (result?.source_status === "cleanup_pending")
        window.alert(
          t("lightbox.deleteConfirm.cleanupPending", { path: result.recovery_path ?? "" })
        );
      if (!disposed && generation === request) setConfirm(false);
    } catch (error) {
      if (!disposed && generation === request)
        setDeleteError(error instanceof Error ? error.message : String(error));
    } finally {
      if (!disposed && generation === request) {
        deletingNow = false;
        setDeleting(false);
      }
    }
  }
  return (
    <Portal mount={document.body}>
      <div
        ref={props.presence.overlayRef}
        data-modal-presence={props.presence.phase()}
        aria-hidden={!props.active || undefined ? "true" : "false"}
        class={`fixed inset-0 z-[55] grid place-items-center ${videoFullscreen() ? "bg-neutral p-0 backdrop-blur-none" : `bg-neutral/62 ${props.selected.kind === "video" ? "p-5" : "p-2 sm:p-5"}`}`}
        data-ui-layer={layer.layerId}
        data-lightbox-backdrop
        onClick={() => {
          if (!props.active || !layer.isTopLayer() || deletingNow || confirm()) return;
          if (sidebar.narrow() && sidebar.open()) closeSidebar();
          else controls.tryClose();
        }}
      >
        <div
          class={[
            "relative min-w-0 overflow-hidden",
            controls.isFullscreen
              ? `h-full min-h-0 w-full rounded-none border-0 shadow-none ${videoFullscreen() ? "bg-neutral" : "bg-base-100"}`
              : [
                  "h-[min(calc(100dvh-40px),1180px)] min-h-0 w-[min(calc(100vw-40px),1800px)]",
                  "rounded-[var(--radius-surface)] border shadow-[var(--shadow-modal)]",
                  "sm:w-[min(calc(100vw-40px),1860px)] sm:rounded-[var(--radius-panel)]",
                  "lg:h-[min(calc(100dvh-40px),1280px)] lg:w-[min(calc(100vw-40px),1920px)]",
                  props.selected.kind === "video"
                    ? "lightbox-shell--video border-white/10"
                    : "border-[var(--border-soft)] bg-[var(--surface-solid)]"
                ].join(" ")
          ].join(" ")}
          inert={!props.active}
          ref={controls.shellRef}
          role="dialog"
          aria-modal="true"
          aria-label={t("lightbox.previewDialog", { name: props.selected.file_name })}
          aria-describedby="lightbox-dialog-description"
          data-lightbox-kind={props.selected.kind}
          tabindex={-1}
          onClick={(event) => event.stopPropagation()}
        >
          <span id="lightbox-dialog-description" class="sr-only">
            {t("lightbox.previewDialogDescription")}
          </span>
          <div
            class={`lightbox-layout relative grid h-full min-h-0 min-w-0 grid-rows-[minmax(0,1fr)] ${!videoFullscreen() && !sidebar.narrow() && sidebar.occupied() ? "grid-cols-[minmax(0,1fr)_340px]" : "grid-cols-[minmax(0,1fr)]"}`}
          >
            <div
              inert={!videoFullscreen() && sidebar.narrow() && sidebar.open()}
              aria-hidden={
                (!videoFullscreen() && sidebar.narrow() && sidebar.open()) || undefined
                  ? "true"
                  : "false"
              }
              class={`lightbox-media-column relative grid h-full min-h-0 min-w-0 overflow-hidden ${videoFullscreen() ? "grid-rows-[minmax(0,1fr)]" : "grid-rows-[minmax(0,1fr)_auto]"} ${props.selected.kind === "video" && !videoFullscreen() ? "pt-11" : ""} ${props.selected.kind === "video" && !videoFullscreen() && sidebar.narrow() && sidebar.occupied() ? "hidden" : ""}`}
            >
              {!videoFullscreen() && !sidebar.open() ? (
                <div
                  class="lightbox-media-controls absolute right-3 top-1.5 z-[5] flex gap-2 transition-opacity duration-200 motion-reduce:transition-none"
                  style={{
                    opacity: activity.visible() ? 1 : 0,
                    "pointer-events": activity.visible() ? undefined : "none"
                  }}
                >
                  <UiIconButton
                    id={TRIGGER_ID}
                    icon="arrow-left"
                    aria-label={t("lightbox.openPanel")}
                    title={t("lightbox.openPanel")}
                    aria-expanded="false"
                    aria-controls="lightbox-sidebar"
                    onClick={openSidebar}
                  />
                  <UiIconButton
                    icon="close"
                    aria-label={t("lightbox.closePreview")}
                    title={t("common.close")}
                    disabled={deleting()}
                    onClick={close}
                  />
                </div>
              ) : null}
              {props.model.favoriteFailed || props.model.groupFailed ? (
                <div class="absolute left-3 top-3 z-[5] grid max-w-[min(28rem,calc(100%-6rem))] gap-2">
                  {props.model.favoriteFailed ? (
                    <UiAlert
                      tone="error"
                      title={t("lightbox.saveFailedTitle")}
                      className="shadow-[var(--shadow-floating)]"
                    >
                      {t("lightbox.favoriteSaveFailed")}
                    </UiAlert>
                  ) : null}
                  {props.model.groupFailed ? (
                    <UiAlert
                      tone="error"
                      title={t("lightbox.saveFailedTitle")}
                      className="shadow-[var(--shadow-floating)]"
                    >
                      {t("lightbox.mediaGroupSaveFailed")}
                    </UiAlert>
                  ) : null}
                </div>
              ) : null}
              {props.model.navigationStatus !== "ready" ? (
                <div
                  class="absolute bottom-3 left-3 z-[6] rounded-lg bg-base-100 p-3 shadow-lg"
                  role="status"
                  data-navigation-status={props.model.navigationStatus}
                >
                  {t(`lightbox.navigation.${props.model.navigationStatus}`)}
                  {props.model.navigationStatus === "failed" ? (
                    <button
                      type="button"
                      class="btn btn-sm ml-3"
                      onClick={() => {
                        void props.model.retryNavigation();
                      }}
                    >
                      {t("gallery.retry")}
                    </button>
                  ) : null}
                </div>
              ) : null}
              <LightboxMediaStage
                selected={props.selected}
                active={props.active}
                detailsFailed={props.model.assetDetailsFailed}
                retryDetails={props.model.retryDetails}
                controls={controls}
                onNativeEscape={handleNativeEscape}
                onPointerActivity={activity.reveal}
                onNativeBounds={handleNativeBounds}
              />
              {!videoFullscreen() && (
                <LightboxActionRail
                  visible={activity.visible()}
                  covered={sidebar.narrow() && sidebar.occupied()}
                  onReveal={activity.reveal}
                  selected={props.selected}
                  isFullscreen={controls.isFullscreen}
                  favoritePending={props.model.favoritePending}
                  onToggleFavorite={() => {
                    void props.model.toggleFavorite();
                  }}
                  onResetZoom={controls.resetZoom}
                  onToggleFullscreen={() => {
                    void controls.toggleFullscreen();
                  }}
                  onOpenDeleteConfirm={() => {
                    openSidebar();
                    setConfirm(true);
                    setDeleteError(null);
                    setInfo(false);
                  }}
                  t={t}
                />
              )}
            </div>
            {!videoFullscreen() && sidebar.narrow() && sidebar.open() ? (
              <div
                class="absolute inset-0 z-[7] bg-neutral/52 backdrop-blur-[1px]"
                data-testid="lightbox-sidebar-scrim"
                onClick={(event) => {
                  event.stopPropagation();
                  closeSidebar();
                }}
              />
            ) : null}
            {!videoFullscreen() && (
              <LightboxToolbar
                client={props.client}
                selected={props.selected}
                mediaGroupKeyEditor={props.groupKey}
                mediaGroupOrderEditor={props.groupOrder}
                isNarrow={sidebar.narrow()}
                sidebarOpen={sidebar.open()}
                sidebarVisible={sidebarVisible()}
                selectedTags={props.tags}
                tagDraft={draft()}
                knownTags={props.knownTags}
                tagInputRef={(node) => {
                  input = node;
                }}
                tagSaving={props.model.tagSaving}
                tagFailed={props.model.tagFailed}
                tagDetailsLoading={props.model.tagDetailsLoading}
                tagDetailsFailed={props.model.tagDetailsFailed}
                tagEditingDisabled={props.model.tagEditingDisabled}
                onRemoveTag={(tag) => {
                  if (!props.model.tagEditingDisabled)
                    props.model.saveTags(props.model.tagEditor.filter((value) => value !== tag));
                }}
                onTagDraftChange={setDraft}
                onAddTag={addTag}
                onRetryTags={props.model.retryTags}
                onRetryTagDetails={props.model.retryDetails}
                onMediaGroupKeyChange={props.model.setMediaGroupKeyEditor}
                onMediaGroupOrderChange={props.model.setMediaGroupOrderEditor}
                onApplyMediaGroup={() => {
                  void props.model.saveMediaGroup();
                }}
                infoPanelOpen={info()}
                onToggleInfo={() => setInfo((value) => !value)}
                onCloseSidebar={closeSidebar}
                onCloseLightbox={close}
                groupPending={props.model.groupPending}
                groupCopyConfirmed={clipboard.confirmed()}
                canCopyMediaGroup={clipboard.canCopy()}
                copyMediaGroupTitle={t(
                  clipboard.confirmed()
                    ? "lightbox.copyMediaGroup.copied"
                    : clipboard.canCopy()
                      ? "lightbox.copyMediaGroup.title"
                      : "lightbox.copyMediaGroup.unavailable"
                )}
                onCopyMediaGroup={() => {
                  void clipboard.copy();
                }}
                deleteConfirmOpen={confirm()}
                deleteSubmitting={deleting()}
                deleteError={deleteError()}
                onDeleteConfirmClose={() => {
                  if (!deletingNow) setConfirm(false);
                }}
                onDeleteConfirmSubmit={() => {
                  void deleteMedia();
                }}
                t={t}
              />
            )}
          </div>
        </div>
      </div>
    </Portal>
  );
}
