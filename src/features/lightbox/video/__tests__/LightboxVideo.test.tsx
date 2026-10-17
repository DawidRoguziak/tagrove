import { cleanup, fireEvent, renderHook, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLibrary } from "../../../../library/createLibrary";
import { createLightbox } from "../../createLightbox";
import LightboxModal from "../../LightboxModal";
import { UiLayerProvider } from "../../../../ui/overlays/UiLayerProvider";
import { SuggestionClient } from "../../../search/suggestions/SuggestionClient";
import { renderSolid } from "../../../../test/renderSolid";
import { settle as act } from "../../../../test/settle";
import { asset, summary, deferred } from "../../__tests__/fixtures";
import type { MpvVideoEvent } from "../../../../platform/tauri/contracts/video";
import { createSignal } from "solid-js";
import { UiModal } from "../../../../ui/overlays/UiModal";
import i18n from "../../../../i18n/core";

const video = vi.hoisted(() => ({
  beginVideoOpen: vi.fn(),
  cancelVideoOpen: vi.fn(),
  openVideo: vi.fn(),
  closeVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  setVideoControlLabels: vi.fn()
}));
const assets = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetDetails: vi.fn(),
  deleteAsset: vi.fn(),
  toMediaSrc: (path: string) => `asset://${path}`
}));
vi.mock("../../../../platform/tauri/video", () => video);
vi.mock("../../../../platform/tauri/assets", () => assets);
let narrow = false;
let resize: Array<() => void> = [];
const videoSummary = (id: number) =>
  summary(id, { kind: "video", preview_path: `/synthetic/${id}.mp4`, width: 640, height: 360 });
async function setup() {
  const root = renderHook(() => {
    const library = createLibrary();
    return { library, model: createLightbox(library) };
  });
  const { library, model } = root.result;
  await act(() => library.query.refresh());
  act(() => model.select(videoSummary(1)));
  const client = new SuggestionClient();
  let openNested = () => {};
  const view = renderSolid(
    () => {
      const [nested, setNested] = createSignal(false);
      openNested = () => setNested(true);
      return (
        <UiLayerProvider>
          <LightboxModal model={model} client={client} knownTags={[]} />
          <UiModal open={nested()} onClose={() => setNested(false)} ariaLabel="Nested">
            <button type="button">Nested action</button>
          </UiModal>
        </UiLayerProvider>
      );
    },
    {}
  );
  await waitFor(() => expect(video.openVideo).toHaveBeenCalledOnce());
  return { model, view, openNested };
}
function emit(event: MpvVideoEvent, call = 0) {
  const callback: (event: MpvVideoEvent) => void = video.openVideo.mock.calls[call][4];
  act(() => callback(event));
}
const player = () => document.querySelector<HTMLElement>("[data-native-video-active]");
async function ready() {
  await waitFor(() => expect(player()).not.toBeNull());
}
beforeEach(async () => {
  vi.resetAllMocks();
  await i18n.changeLanguage("en");
  narrow = false;
  resize = [];
  vi.stubGlobal(
    "ResizeObserver",
    class implements ResizeObserver {
      constructor(private callback: ResizeObserverCallback) {}
      observe() {
        resize.push(() => this.callback([], this));
      }
      unobserve() {}
      disconnect() {}
    }
  );
  window.matchMedia = vi.fn((query: string) => ({
    matches: query.includes("767px") ? narrow : false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: () => true
  }));
  for (const mock of Object.values(video)) mock.mockResolvedValue(undefined);
  let request = 0;
  video.beginVideoOpen.mockImplementation(async () => ++request);
  video.openVideo.mockImplementation(async (_id, request) => request * 10);
  assets.startAssetQuery.mockResolvedValue({
    status: "ready",
    session_id: 1,
    revision: 1,
    offset: 0,
    total: 3,
    items: [videoSummary(1), videoSummary(2), videoSummary(3)]
  });
  assets.getAssetDetails.mockImplementation(async (id: number) =>
    asset(id, { ...videoSummary(id), path: `/synthetic/${id}.mp4` })
  );
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("Solid native video lightbox", () => {
  it("routes native Escape through confirmation, drawer and lightbox priority", async () => {
    narrow = true;
    const { model } = await setup();
    await ready();
    act(() => fireEvent.click(screen.getByRole("button", { name: "Delete media" })));
    act(() => resize.forEach((callback) => callback()));
    await screen.findByTestId("lightbox-delete-confirm-dialog");
    emit({ session_id: 10, type: "escape" });
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).toBeNull();
    expect(model.selected?.id).toBe(1);
    expect(document.getElementById("lightbox-sidebar-close-button")).toHaveAttribute("aria-expanded", "true");
    emit({ session_id: 10, type: "escape" });
    expect(document.getElementById("lightbox-sidebar-close-button")).toHaveAttribute("aria-expanded", "false");
    expect(model.selected?.id).toBe(1);
    emit({ session_id: 10, type: "escape" });
    expect(model.selected).toBeNull();
    expect(video.closeVideo).toHaveBeenCalledWith(10);
  });

  it("keeps pending deletion locked against native Escape", async () => {
    const pending = deferred<never>();
    assets.deleteAsset.mockReturnValueOnce(pending.promise);
    const { model } = await setup();
    await ready();
    act(() => fireEvent.click(screen.getByRole("button", { name: "Delete media" })));
    act(() => fireEvent.input(document.getElementById("lightbox-delete-confirm-input")!, {
      target: { value: "Yes" }
    }));
    act(() => fireEvent.click(screen.getByRole("button", { name: "Confirm" })));
    expect(assets.deleteAsset).toHaveBeenCalledOnce();
    emit({ session_id: 10, type: "escape" });
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();
    expect(model.selected?.id).toBe(1);
    await act(async () => pending.reject(new Error("synthetic delete failure")));
    emit({ session_id: 10, type: "escape" });
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).toBeNull();
    expect(model.selected?.id).toBe(1);
  });

  it("rejects native Escape behind a higher layer and from retired playback", async () => {
    const { model, openNested } = await setup();
    await ready();
    act(openNested);
    emit({ session_id: 10, type: "escape" });
    expect(screen.getByRole("dialog", { name: "Nested" })).toBeInTheDocument();
    expect(model.selected?.id).toBe(1);
    act(() => fireEvent.keyDown(window, { key: "Escape" }));
    act(() => model.select(videoSummary(2)));
    await waitFor(() => expect(player()).toHaveAttribute("data-native-session", "20"));
    emit({ session_id: 10, type: "escape" });
    emit({ session_id: 10, type: "escape" }, 1);
    expect(model.selected?.id).toBe(2);
    emit({ session_id: 20, type: "escape" }, 1);
    expect(model.selected).toBeNull();
  });

  it("opens by asset ID with translated native labels and no HTML video", async () => {
    await setup();
    await ready();
    expect(video.openVideo).toHaveBeenCalledWith(
      1,
      1,
      { x: 0, y: 0, width: 0, height: 0 },
      expect.objectContaining({ play: "Play", seek: "Seek" }),
      expect.any(Function)
    );
    expect(document.querySelector("video")).toBeNull();
    expect(screen.getByRole("dialog")).toHaveClass("lightbox-shell--video");
    expect(document.querySelector("[data-lightbox-backdrop]")).toHaveClass("p-5");
    await act(() => i18n.changeLanguage("pl"));
    await waitFor(() =>
      expect(video.setVideoControlLabels).toHaveBeenLastCalledWith(
        10,
        expect.objectContaining({ play: i18n.t("lightbox.videoControls.play") })
      )
    );
    expect(video.openVideo).toHaveBeenCalledOnce();
  });

  it("publishes explicit snapshot attribute values and preserves playback on control errors", async () => {
    await setup();
    await ready();
    const node = player();
    emit({
      session_id: 10,
      type: "snapshot",
      state: {
        session_id: 10,
        duration: 8,
        current_time: 2.5,
        paused: false,
        seeking: false,
        buffering: true,
        volume: 0.5,
        muted: false,
        rate: 1.5,
        fullscreen: false
      }
    });
    expect(node).toHaveAttribute("data-native-paused", "false");
    expect(node).toHaveAttribute("data-native-seeking", "false");
    expect(node).toHaveAttribute("data-native-buffering", "true");
    expect(node).toHaveAttribute("data-native-muted", "false");
    expect(node).toHaveAttribute("data-native-time", "2.5");
    expect(node).toHaveAttribute("data-native-volume", "0.5");
    expect(node).toHaveAttribute("data-native-rate", "1.5");
    emit({ session_id: 10, type: "controlError", message: "seek rejected" });
    expect(screen.getByRole("alert")).toHaveTextContent(
      i18n.t("lightbox.videoControls.controlError")
    );
    act(() =>
      fireEvent.click(within(screen.getByRole("alert")).getByRole("button", { name: "Close" }))
    );
    expect(screen.queryByRole("alert")).toBeNull();
    expect(player()).toBe(node);
    expect(video.closeVideo).not.toHaveBeenCalled();
  });

  it("waits for native placement before revealing or focusing the sidebar", async () => {
    const bounds = deferred<void>();
    video.setVideoBounds.mockReturnValueOnce(bounds.promise);
    await setup();
    await ready();
    const panel = document.getElementById("lightbox-sidebar");
    expect(panel).toHaveAttribute("aria-hidden", "true");
    expect(panel).toHaveAttribute("inert");
    await act(async () => bounds.resolve());
    await waitFor(() => expect(panel).toHaveAttribute("aria-hidden", "false"));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Close asset panel" })).toHaveFocus()
    );
  });

  it.each([
    false,
    true
  ])("consumes fullscreen Escape without closing or replacing playback, narrow=%s", async (isNarrow) => {
    narrow = isNarrow;
    const { model } = await setup();
    await ready();
    if (narrow) {
      act(() => fireEvent.click(screen.getByRole("button", { name: "Open asset panel" })));
      act(() => resize.forEach((callback) => callback()));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Close asset panel" })).toBeInTheDocument()
      );
    }
    const node = player();
    const column = document.querySelector(".lightbox-media-column");
    expect(column).toHaveClass("pt-11");
    emit({ session_id: 10, type: "fullscreen", fullscreen: true });
    expect(column).not.toHaveClass("pt-11");
    expect(column).not.toHaveClass("hidden");
    expect(column).not.toHaveAttribute("inert");
    expect(screen.queryByRole("button", { name: "Close preview" })).toBeNull();
    const later = vi.fn();
    window.addEventListener("keydown", later, true);
    const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    act(() => node?.dispatchEvent(event));
    window.removeEventListener("keydown", later, true);
    expect(event.defaultPrevented).toBe(true);
    expect(later).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(video.controlVideo).toHaveBeenCalledExactlyOnceWith(10, { type: "toggleFullscreen" })
    );
    expect(model.selected?.id).toBe(1);
    emit({ session_id: 10, type: "fullscreen", fullscreen: false });
    expect(column).toHaveClass("pt-11");
    expect(document.getElementById("lightbox-sidebar-close-button")).toHaveAttribute(
      "aria-expanded",
      "true"
    );
    expect(player()).toBe(node);
    expect(video.openVideo).toHaveBeenCalledOnce();
    expect(video.closeVideo).not.toHaveBeenCalled();
  });

  it("restores image layout when navigating away from a fullscreen native session", async () => {
    const { model } = await setup();
    await ready();
    emit({ session_id: 10, type: "fullscreen", fullscreen: true });
    expect(screen.getByRole("dialog")).toHaveClass("rounded-none");
    assets.getAssetDetails.mockResolvedValueOnce(asset(4));
    act(() => model.select(summary(4)));
    await screen.findByRole("img");
    expect(screen.getByRole("dialog")).not.toHaveClass("rounded-none");
    expect(screen.getByRole("dialog")).toHaveAttribute("data-lightbox-kind", "image");
    expect(video.closeVideo).toHaveBeenCalledWith(10);
  });

  it("keeps seek and pause keys on the video and suppresses repeats and modifiers", async () => {
    const { model } = await setup();
    await ready();
    const node = player();
    if (!node) throw Error("Missing native player");
    act(() => fireEvent.keyDown(node, { key: "ArrowRight" }));
    act(() => fireEvent.keyDown(node, { key: "ArrowLeft" }));
    act(() => fireEvent.keyDown(node, { key: " " }));
    act(() => fireEvent.keyDown(node, { key: "Enter", repeat: true }));
    act(() => fireEvent.keyDown(node, { key: "ArrowRight", ctrlKey: true }));
    await waitFor(() => expect(video.controlVideo).toHaveBeenCalledTimes(3));
    expect(video.controlVideo.mock.calls).toEqual([
      [10, { type: "seekRelative", seconds: 5 }],
      [10, { type: "seekRelative", seconds: -5 }],
      [10, { type: "togglePause" }]
    ]);
    expect(model.selected?.id).toBe(1);
    act(() => fireEvent.click(node));
    await waitFor(() => expect(video.controlVideo).toHaveBeenCalledTimes(4));
    expect(node).toHaveFocus();
  });

  it("closes playback on logical close while retaining the exiting dialog", async () => {
    const { model } = await setup();
    await ready();
    act(() => model.close());
    expect(video.closeVideo).toHaveBeenCalledWith(10);
    expect(video.cancelVideoOpen).toHaveBeenCalledWith(1);
    expect(player()).toBeNull();
    expect(screen.getByRole("dialog", { hidden: true })).toHaveAttribute("inert");
  });

  it("cancels a pending open on close and closes its late session", async () => {
    const open = deferred<number>();
    video.openVideo.mockReturnValueOnce(open.promise);
    const { model } = await setup();
    act(() => model.close());
    expect(video.cancelVideoOpen).toHaveBeenCalledWith(1);
    await act(async () => open.resolve(10));
    expect(video.closeVideo).toHaveBeenCalledWith(10);
    expect(player()).toBeNull();
  });

  it("retries failed native opening and rejects late events after A to B to A", async () => {
    const originalError = console.error;
    vi.spyOn(console, "error").mockImplementation((...args: unknown[]) => {
      if (!String(args[0]).startsWith("[native-video]")) originalError(...args);
    });
    video.openVideo.mockRejectedValueOnce(Error("decoder failed"));
    const { model } = await setup();
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not play video");
    act(() =>
      fireEvent.click(
        within(screen.getByTestId("lightbox-media-error")).getByRole("button", { name: "Retry" })
      )
    );
    await ready();
    act(() => model.select(videoSummary(2)));
    await waitFor(() => expect(player()).toHaveAttribute("data-native-session", "30"));
    act(() => model.select(videoSummary(1)));
    await waitFor(() => expect(player()).toHaveAttribute("data-native-session", "40"));
    emit({ session_id: 20, type: "error", message: "retired decoder" }, 1);
    expect(screen.queryByTestId("lightbox-media-error")).toBeNull();
    expect(player()).toHaveAttribute("data-native-session", "40");
  });

  it("hides a narrow native surface and waits for the drawer exit before restoring focus", async () => {
    narrow = true;
    await setup();
    await ready();
    act(() => fireEvent.click(screen.getByRole("button", { name: "Open asset panel" })));
    const column = document.querySelector(".lightbox-media-column");
    expect(column).toHaveClass("hidden");
    expect(document.getElementById("lightbox-sidebar")).toHaveAttribute("aria-hidden", "true");
    act(() => resize.forEach((callback) => callback()));
    const collapse = await screen.findByRole("button", { name: "Close asset panel" });
    await waitFor(() => expect(collapse).toHaveFocus());
    vi.useFakeTimers();
    act(() => fireEvent.click(collapse, { detail: 0 }));
    act(() => vi.advanceTimersByTime(100));
    const trigger = screen.getByRole("button", { name: "Open asset panel" });
    expect(column).toHaveClass("hidden");
    expect(trigger).not.toHaveFocus();
    act(() => vi.advanceTimersByTime(100));
    act(() => vi.advanceTimersByTime(20));
    expect(column).not.toHaveClass("hidden");
    expect(trigger).toHaveFocus();
    expect(video.openVideo).toHaveBeenCalledOnce();
  });
});
