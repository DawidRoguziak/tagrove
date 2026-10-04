import { createSignal, onSettled, For, Show } from "solid-js";
import { Portal } from "@solidjs/web";
import { isTauri } from "../platform/tauri/window";
import { getCurrentWindow } from "../platform/tauri/window";
import { useLanguage } from "../i18n/solid";
import { UiButton } from "../ui/controls/UiButton";
import { UiIcon } from "../ui/controls/UiIcon";

type ResizeDirection = Parameters<ReturnType<typeof getCurrentWindow>["startResizeDragging"]>[0];
const resizeEdges = [
  "North",
  "South",
  "East",
  "West",
  "NorthEast",
  "NorthWest",
  "SouthEast",
  "SouthWest"
] satisfies ResizeDirection[];

function reportWindowError(error: unknown) {
  console.error("Window operation failed", error);
}

export function WindowControls() {
  const { t } = useLanguage();
  const appWindow = isTauri() ? getCurrentWindow() : null;
  const [state, setState] = createSignal({ maximized: false, fullscreen: false });

  onSettled(() => {
    if (!appWindow) return;
    let disposed = false;
    let syncGeneration = 0;
    let unlisten: (() => void) | undefined;
    async function sync() {
      if (!appWindow) return;
      const generation = ++syncGeneration;
      const [maximized, fullscreen] = await Promise.all([
        appWindow.isMaximized(),
        appWindow.isFullscreen()
      ]);
      if (!disposed && generation === syncGeneration) setState({ maximized, fullscreen });
    }
    void appWindow
      .onResized(() => {
        void sync().catch(reportWindowError);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(reportWindowError);
    void sync().catch(reportWindowError);
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  if (!appWindow) return null;

  return (
    <>
      <div
        data-tauri-drag-region="false"
        role="group"
        aria-label={t("window.controls")}
        class="window-controls"
      >
        <UiButton
          variant="ghost"
          aria-label={t("window.minimize")}
          title={t("window.minimize")}
          onClick={() => {
            void appWindow.minimize().catch(reportWindowError);
          }}
        >
          <UiIcon name="window-minimize" />
        </UiButton>
        <UiButton
          variant="ghost"
          aria-label={t(state().maximized ? "window.restore" : "window.maximize")}
          title={t(state().maximized ? "window.restore" : "window.maximize")}
          onClick={() => {
            void appWindow.toggleMaximize().catch(reportWindowError);
          }}
        >
          <UiIcon name={state().maximized ? "window-restore" : "window-maximize"} />
        </UiButton>
        <UiButton
          variant="ghost"
          className="window-close"
          aria-label={t("window.close")}
          title={t("window.close")}
          onClick={() => {
            void appWindow.close().catch(reportWindowError);
          }}
        >
          <UiIcon name="close" />
        </UiButton>
      </div>
      <Show when={!state().maximized && !state().fullscreen}>
        <Portal>
          <div aria-hidden="true" class="window-resize-edges">
            <For each={resizeEdges}>
              {(direction) => (
                <div
                  data-resize-direction={direction}
                  onMouseDown={(event) => {
                    if (event.button !== 0) return;
                    event.preventDefault();
                    void appWindow.startResizeDragging(direction).catch(reportWindowError);
                  }}
                />
              )}
            </For>
          </div>
        </Portal>
      </Show>
    </>
  );
}
