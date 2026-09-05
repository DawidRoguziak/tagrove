import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { UiButton } from "../UI/UiButton";
import { UiIcon } from "../UI/UiIcon";

type ResizeDirection = Parameters<ReturnType<typeof getCurrentWindow>["startResizeDragging"]>[0];
const resizeEdges = ["North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest"] satisfies ResizeDirection[];

function reportWindowError(error: unknown) {
  console.error("Window operation failed", error);
}

export function WindowControls() {
  const { t } = useTranslation();
  const [appWindow] = useState(() => isTauri() ? getCurrentWindow() : null);
  const [state, setState] = useState({ maximized: false, fullscreen: false });

  useEffect(() => {
    if (!appWindow) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    async function sync() {
      if (!appWindow) return;
      const [maximized, fullscreen] = await Promise.all([appWindow.isMaximized(), appWindow.isFullscreen()]);
      if (!disposed) setState({ maximized, fullscreen });
    }
    void appWindow.onResized(() => { void sync().catch(reportWindowError); }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(reportWindowError);
    void sync().catch(reportWindowError);
    return () => { disposed = true; unlisten?.(); };
  }, [appWindow]);

  if (!appWindow) return null;

  return (
    <>
      <div data-tauri-drag-region="false" role="group" aria-label={t("window.controls")} className="window-controls">
        <UiButton variant="ghost" aria-label={t("window.minimize")} title={t("window.minimize")}
          onClick={() => { void appWindow.minimize().catch(reportWindowError); }}>
          <UiIcon name="window-minimize" />
        </UiButton>
        <UiButton variant="ghost" aria-label={t(state.maximized ? "window.restore" : "window.maximize")}
          title={t(state.maximized ? "window.restore" : "window.maximize")}
          onClick={() => { void appWindow.toggleMaximize().catch(reportWindowError); }}>
          <UiIcon name={state.maximized ? "window-restore" : "window-maximize"} />
        </UiButton>
        <UiButton variant="ghost" className="window-close" aria-label={t("window.close")} title={t("window.close")}
          onClick={() => { void appWindow.close().catch(reportWindowError); }}>
          <UiIcon name="close" />
        </UiButton>
      </div>
      {!state.maximized && !state.fullscreen && createPortal(
        <div aria-hidden="true" className="window-resize-edges">
          {resizeEdges.map((direction) => <div key={direction} data-resize-direction={direction}
            onMouseDown={(event) => {
              if (event.button !== 0) return;
              event.preventDefault();
              void appWindow.startResizeDragging(direction).catch(reportWindowError);
            }} />)}
        </div>, document.body)}
    </>
  );
}
