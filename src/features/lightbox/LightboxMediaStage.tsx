import { createEffect, createMemo, createSignal, onCleanup, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
import type { SelectedAsset } from "./types";
import { toMediaSrc } from "../../platform/tauri/assets";
import { useLanguage } from "../../i18n/solid";
import { UiButton } from "../../ui/controls/UiButton";
import type { createImageControls } from "./image/createImageControls";

import { LightboxVideoPlayer } from "./video/LightboxVideoPlayer";
import type { VideoBounds } from "../../platform/tauri/contracts/video";

export function LightboxMediaStage(props: {
  selected: SelectedAsset;
  active: boolean;
  detailsFailed: boolean;
  retryDetails: () => void;
  controls: ReturnType<typeof createImageControls>;
  onNativeEscape?: () => void;
  onPointerActivity?: () => void;
  onNativeBounds?: (bounds: VideoBounds) => void;
}) {
  const { t } = useLanguage();
  const source = createMemo(
    () => `${props.selected.id}\0${props.selected.kind}\0${props.selected.path ?? ""}`
  );
  const style = (): JSX.CSSProperties | undefined => {
    const size = props.controls.mediaDisplaySize;
    return size.width > 0 && size.height > 0
      ? {
          width: `calc(${size.width}px * var(--lightbox-image-zoom, 1))`,
          height: `calc(${size.height}px * var(--lightbox-image-zoom, 1))`,
          "max-width": "none",
          "max-height": "none"
        }
      : undefined;
  };
  return (
    <div
      class={`relative h-full min-h-0 min-w-0 w-full overflow-hidden [contain:paint] ${props.selected.kind === "video" ? "lightbox-media-stage--video" : "bg-[var(--surface-muted)]"}`}
      data-lightbox-media-stage={props.selected.kind}
      ref={props.controls.viewportRef}
    >
      <Show when={source()} keyed>
        {(_source) => {
          const [failed, setFailed] = createSignal(false);
          let disposed = false;
          onCleanup(() => {
            disposed = true;
          });
          createEffect(
            () => props.selected.kind === "video" && (props.selected.path === null || failed()),
            (cleared) => {
              if (cleared) props.onNativeBounds?.({ x: 0, y: 0, width: 0, height: 0 });
            }
          );
          return (
            <Show
              when={props.selected.path !== null}
              fallback={
                props.detailsFailed ? (
                  <div
                    role="alert"
                    data-testid="lightbox-details-error"
                    class="flex h-full w-full items-center justify-center p-6"
                  >
                    <div class="grid gap-3 rounded-[var(--radius-control)] border border-warning/52 bg-warning/14 px-5 py-4 text-center text-sm font-semibold text-base-content">
                      <span>{t("lightbox.assetDetailsLoadFailed")}</span>
                      <UiButton onClick={props.retryDetails}>
                        {t("lightbox.retryAssetDetails")}
                      </UiButton>
                    </div>
                  </div>
                ) : (
                  <div
                    class="flex h-full w-full items-center justify-center"
                    data-testid="lightbox-details-loading"
                  >
                    <span class="h-[34px] w-[34px] animate-spin rounded-full border-[3px] border-base-content/25 border-t-primary" />
                  </div>
                )
              }
            >
              <Show
                when={!failed()}
                fallback={
                  <div class="flex h-full w-full items-center justify-center p-6">
                    <div
                      role="alert"
                      data-testid="lightbox-media-error"
                      class="rounded-[var(--radius-control)] border border-error/52 bg-error/14 px-5 py-4 text-center text-sm font-semibold text-base-content"
                    >
                      {t(
                        props.selected.kind === "video"
                          ? "lightbox.mediaError.video"
                          : "lightbox.mediaError.image"
                      )}
                      {props.selected.kind === "video" && (
                        <div class="mt-3">
                          <UiButton onClick={() => setFailed(false)}>{t("gallery.retry")}</UiButton>
                        </div>
                      )}
                    </div>
                  </div>
                }
              >
                {props.selected.kind === "video" ? (
                  <div class="flex h-full w-full items-center justify-center overflow-hidden">
                    <Show when={props.active}>
                      <LightboxVideoPlayer
                        assetId={props.selected.id}
                        generation={0}
                        title={props.selected.path ?? ""}
                        style={
                          props.controls.isFullscreen
                            ? undefined
                            : {
                                width:
                                  props.controls.mediaDisplaySize.width > 0
                                    ? `${props.controls.mediaDisplaySize.width}px`
                                    : undefined,
                                height:
                                  props.controls.mediaDisplaySize.height > 0
                                    ? `${props.controls.mediaDisplaySize.height}px`
                                    : undefined
                              }
                        }
                        aspectRatio={
                          props.controls.mediaDisplaySize.width > 0 &&
                          props.controls.mediaDisplaySize.height > 0
                            ? `${props.controls.mediaDisplaySize.width} / ${props.controls.mediaDisplaySize.height}`
                            : props.selected.width && props.selected.height
                              ? `${props.selected.width} / ${props.selected.height}`
                              : "16 / 9"
                        }
                        layoutRoot={props.controls.viewport() ?? null}
                        playerRef={props.controls.videoPlayerRef}
                        onLoadedMetadata={props.controls.onVideoLoadedMetadata}
                        onFullscreenChange={props.controls.onVideoFullscreenChange}
                        onEscape={props.onNativeEscape}
                        onPointerActivity={props.onPointerActivity}
                        onNativeBounds={props.onNativeBounds}
                        onError={() => setFailed(true)}
                      />
                    </Show>
                  </div>
                ) : (
                  <div
                    class={`flex h-full w-full touch-none items-center justify-center overflow-hidden ${props.controls.isZoomed ? (props.controls.isDragging ? "cursor-grabbing" : "cursor-grab") : ""}`}
                    onPointerDown={props.controls.onPointerDown}
                    onPointerUp={props.controls.onPointerEnd}
                    onPointerCancel={props.controls.onPointerEnd}
                    onLostPointerCapture={props.controls.onPointerEnd}
                  >
                    <img
                      data-testid="lightbox-image"
                      crossorigin="anonymous"
                      ref={props.controls.imageRef}
                      src={toMediaSrc(props.selected.path ?? "")}
                      alt={props.selected.file_name}
                      style={style()}
                      class={`max-h-full max-w-full shrink-0 select-none object-contain ${props.controls.isZoomed ? (props.controls.isDragging ? "cursor-grabbing" : "cursor-grab") : "cursor-zoom-in"}`}
                      onLoad={(event) => {
                        if (!disposed && source() === _source) props.controls.onLoad(event);
                      }}
                      onError={() => setFailed(true)}
                      onClick={props.controls.onClick}
                      draggable={false}
                    />
                  </div>
                )}
              </Show>
            </Show>
          );
        }}
      </Show>
      {props.selected.path !== null && props.detailsFailed ? (
        <div
          class={`absolute left-4 ${props.selected.kind === "video" ? "bottom-[88px]" : "bottom-4"} z-[3] flex items-center gap-3 rounded-[var(--radius-control)] border border-warning/52 bg-base-100/92 px-3 py-2 text-xs shadow-[var(--shadow-floating)]`}
          role="alert"
        >
          <span>{t("lightbox.assetDetailsLoadFailed")}</span>
          <UiButton className="btn-sm" onClick={props.retryDetails}>
            {t("lightbox.retryAssetDetails")}
          </UiButton>
        </div>
      ) : null}
    </div>
  );
}
