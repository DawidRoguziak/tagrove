import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../types";
import { LightboxModal } from "../LightboxModal";
import { UiLayerProvider } from "../../UI/UiLayerProvider";
import type { MpvVideoEvent } from "../mpvVideoTypes";
import styles from "../../../styles.css?raw";

const apiMocks = vi.hoisted(() => ({
  beginVideoOpen: vi.fn(),
  cancelVideoOpen: vi.fn(),
  setVideoControlLabels: vi.fn(),
  openVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  closeVideo: vi.fn()
}));

vi.mock("../../../api", () => ({
  beginVideoOpen: apiMocks.beginVideoOpen,
  cancelVideoOpen: apiMocks.cancelVideoOpen,
  setVideoControlLabels: apiMocks.setVideoControlLabels,
  openVideo: apiMocks.openVideo,
  setVideoBounds: apiMocks.setVideoBounds,
  controlVideo: apiMocks.controlVideo,
  closeVideo: apiMocks.closeVideo,
  toMediaSrc: (path: string) => `media://${path}`
}));

const selectedAsset: SelectedAsset = {
  id: 1,
  file_name: "1.jpg",
  preview_path: null,
  kind: "image",
  modified_at: 1700000000,
  width: 1200,
  height: 800,
  duration_ms: null,
  thumb_path: null,
  is_favorite: false,
  media_group_key: null,
  media_group_order: null,
  path: "C:/media/1.jpg",
  size_bytes: 1024,
  tags: []
};

const selectedVideoAsset: SelectedAsset = {
  ...selectedAsset,
  path: "C:/media/1.mp4",
  kind: "video",
  duration_ms: 20_000
};
const originalMatchMedia = window.matchMedia;
const originalResizeObserver = window.ResizeObserver;

function stubMatchMedia(matches: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn(() => ({
      matches,
      media: "(max-width: 767px)",
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  });
}

describe("LightboxModal", () => {
  it("shows navigation recovery states and delegates Retry", async () => {
    const onRetryNavigation = vi.fn();
    const props = {
      selected: selectedAsset, tagEditor: [], knownTags: [],
      onTagEditorChange: vi.fn(), onSaveTags: vi.fn(), onNavigatePrevious: vi.fn(),
      onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn(), onRetryNavigation
    };
    const { rerender } = render(<LightboxModal {...props} navigationStatus="resolving" />);
    expect(screen.getByRole("status")).toHaveTextContent("Finding this photo in the results");
    rerender(<LightboxModal {...props} navigationStatus="failed" />);
    await userEvent.click(screen.getByRole("button", { name: /^Retry$/ }));
    expect(onRetryNavigation).toHaveBeenCalledTimes(1);
    rerender(<LightboxModal {...props} navigationStatus="missing" />);
    expect(screen.getByRole("status")).toHaveTextContent("This photo is no longer in the results");
    rerender(<LightboxModal {...props} navigationStatus="ready" />);
    expect(screen.queryByText("Finding this photo in the results…")).not.toBeInTheDocument();
  });

  beforeEach(() => {
    // Load the real lightbox timing rule; jsdom cannot parse the full Tailwind stylesheet.
    const style = document.createElement("style");
    style.dataset.lightboxTiming = "";
    const rule = styles.match(/\[data-lightbox-backdrop\]\s*\{[^}]+\}/)?.[0];
    if (!rule) throw new Error("Missing lightbox timing rule");
    style.textContent = rule;
    document.head.append(style);
    stubMatchMedia(false);
    apiMocks.beginVideoOpen.mockReset().mockResolvedValue(1);
    apiMocks.cancelVideoOpen.mockReset().mockResolvedValue(undefined);
    apiMocks.setVideoControlLabels.mockReset().mockResolvedValue(undefined);
    apiMocks.openVideo.mockReset().mockResolvedValue(1);
    apiMocks.setVideoBounds.mockReset().mockResolvedValue(undefined);
    apiMocks.controlVideo.mockReset().mockResolvedValue(undefined);
    apiMocks.closeVideo.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    document.querySelector("style[data-lightbox-timing]")?.remove();
    vi.restoreAllMocks();
    vi.useRealTimers();
    window.ResizeObserver = originalResizeObserver;
    Object.defineProperty(window, "matchMedia", { configurable: true, value: originalMatchMedia });
  });

  it("does not replay the fade on navigation and retains the closing image and editor props", () => {
    vi.useFakeTimers();
    const props = {
      selected: selectedAsset, tagEditor: ["retained-tag"], mediaGroupKeyEditor: "retained-group",
      onTagEditorChange: vi.fn(), onSaveTags: vi.fn(), knownTags: [],
      onNavigatePrevious: vi.fn(), onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn()
    };
    const { rerender } = render(<LightboxModal {...props} />);
    const overlay = document.querySelector("[data-lightbox-backdrop]");
    act(() => vi.advanceTimersByTime(74));
    expect(overlay).toHaveAttribute("data-modal-presence", "opening");
    act(() => vi.advanceTimersByTime(1));
    expect(overlay).toHaveAttribute("data-modal-presence", "open");
    const next = { ...selectedAsset, id: 2, path: "/synthetic/next.gif", file_name: "next.gif", kind: "gif" as const };
    rerender(<LightboxModal {...props} selected={next} />);
    expect(document.querySelector("[data-lightbox-backdrop]")).toBe(overlay);
    expect(overlay).toHaveAttribute("data-modal-presence", "open");
    rerender(<LightboxModal {...props} selected={null} tagEditor={[]} mediaGroupKeyEditor="" />);
    expect(overlay).toHaveAttribute("data-modal-presence", "closing");
    expect(overlay).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("lightbox-image")).toHaveAttribute("src", "media:///synthetic/next.gif");
    expect(screen.getByDisplayValue("retained-group")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(props.onNavigateNext).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(49));
    expect(overlay).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(overlay).not.toBeInTheDocument();
  });

  it("keeps a rapidly reactivated lightbox open after the old exit deadline", () => {
    vi.useFakeTimers();
    const props = {
      selected: selectedAsset, tagEditor: [], knownTags: [],
      onTagEditorChange: vi.fn(), onSaveTags: vi.fn(), onNavigatePrevious: vi.fn(),
      onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn()
    };
    const { rerender } = render(<LightboxModal {...props} />);
    act(() => vi.advanceTimersByTime(25));
    rerender(<LightboxModal {...props} selected={null} />);
    act(() => vi.advanceTimersByTime(25));
    rerender(<LightboxModal {...props} />);
    const overlay = document.querySelector("[data-lightbox-backdrop]");
    act(() => vi.advanceTimersByTime(74));
    expect(overlay).toHaveAttribute("data-modal-presence", "opening");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(overlay).toHaveAttribute("data-modal-presence", "open");
  });

  it("opens and closes immediately with reduced motion", () => {
    stubMatchMedia(true);
    const props = {
      selected: selectedAsset, tagEditor: [], knownTags: [],
      onTagEditorChange: vi.fn(), onSaveTags: vi.fn(), onNavigatePrevious: vi.fn(),
      onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn()
    };
    const { rerender } = render(<LightboxModal {...props} />);
    expect(document.querySelector("[data-lightbox-backdrop]")).toHaveAttribute("data-modal-presence", "open");
    rerender(<LightboxModal {...props} selected={null} />);
    expect(document.querySelector("[data-lightbox-backdrop]")).toBeNull();
    rerender(<LightboxModal {...props} />);
    expect(document.querySelector("[data-lightbox-backdrop]")).toHaveAttribute("data-modal-presence", "open");
  });

  it("closes native video immediately while retaining the dialog for its exit", async () => {
    const props = {
      selected: selectedVideoAsset, tagEditor: [], knownTags: [],
      onTagEditorChange: vi.fn(), onSaveTags: vi.fn(), onNavigatePrevious: vi.fn(),
      onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn()
    };
    const { rerender } = render(<LightboxModal {...props} />);
    await waitFor(() => expect(apiMocks.openVideo).toHaveBeenCalled());
    vi.useFakeTimers();
    rerender(<LightboxModal {...props} selected={null} />);
    expect(apiMocks.closeVideo).toHaveBeenCalledWith(1);
    expect(document.querySelector("[data-lightbox-video-player]")).toBeNull();
    const overlay = document.querySelector("[data-lightbox-backdrop]");
    expect(overlay).toHaveAttribute("data-modal-presence", "closing");
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(props.onNavigateNext).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(49));
    expect(overlay).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(overlay).not.toBeInTheDocument();
  });

  it.each([false, true])("places Close in the open panel header and beside reopen when collapsed, narrow=%s", async (narrow) => {
    stubMatchMedia(narrow);
    const onClose = vi.fn();
    render(<LightboxModal selected={selectedAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()} onClose={onClose} />);
    if (narrow) await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    const collapse = screen.getByRole("button", { name: "Close asset panel" });
    expect(collapse.closest("header")).toContainElement(screen.getByRole("button", { name: "Close preview" }));
    expect(document.querySelector(".lightbox-media-controls")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Close preview" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    await userEvent.click(collapse);
    const floating = document.querySelector(".lightbox-media-controls");
    expect(floating).toContainElement(screen.getByRole("button", { name: "Open asset panel" }));
    expect(floating).toContainElement(screen.getByRole("button", { name: "Close preview" }));
    await userEvent.click(screen.getByRole("button", { name: "Close preview" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it.each([false, true])("disables header Close during pending deletion, narrow=%s", async (narrow) => {
    stubMatchMedia(narrow);
    let finishDelete: () => void = () => {};
    const deletion = new Promise<void>((resolve) => { finishDelete = resolve; });
    const onClose = vi.fn();
    render(<LightboxModal selected={selectedAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()} onClose={onClose}
      onDeleteMedia={() => deletion} />);
    await userEvent.click(screen.getByRole("button", { name: "Delete media" }));
    await userEvent.type(screen.getByLabelText(/Type/), "Yes");
    await userEvent.click(screen.getByRole("button", { name: "Confirm" }));
    const close = screen.getByRole("button", { name: "Close preview" });
    expect(close).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close asset panel" })).toBeDisabled();
    await userEvent.click(close);
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => { finishDelete(); await deletion; });
  });

  it("fades mounted controls after idle, keeps the panel visible, and executes the first shortcut", () => {
    vi.useFakeTimers();
    const onNavigateNext = vi.fn();
    render(<UiLayerProvider><LightboxModal selected={selectedAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={onNavigateNext} onToggleFavorite={vi.fn()} onClose={vi.fn()} /></UiLayerProvider>);
    const row = screen.getByTestId("lightbox-action-rail").parentElement;
    const panel = document.getElementById("lightbox-sidebar");
    act(() => vi.advanceTimersByTime(2999));
    expect(row).toHaveStyle({ opacity: "1" });
    act(() => vi.advanceTimersByTime(1));
    expect(row).toHaveStyle({ opacity: "0", pointerEvents: "none" });
    expect(panel).toHaveAttribute("aria-hidden", "false");
    expect(screen.getByRole("button", { name: "Close preview" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    act(() => vi.advanceTimersByTime(20));
    const trigger = screen.getByRole("button", { name: "Open asset panel" });
    const floating = trigger.parentElement;
    expect(trigger).toHaveFocus();
    act(() => vi.advanceTimersByTime(3000));
    expect(floating).toHaveStyle({ opacity: "0", pointerEvents: "none" });
    expect(trigger).toHaveFocus();
    fireEvent.keyDown(trigger, { key: "ArrowRight" });
    expect(onNavigateNext).toHaveBeenCalledOnce();
    expect(floating).toHaveStyle({ opacity: "1" });
    expect(row).toHaveStyle({ opacity: "1" });
    expect(screen.getByTestId("lightbox-action-rail").parentElement).toBe(row);
    expect(row).toHaveClass("duration-200", "motion-reduce:transition-none");
    expect(floating).toHaveClass("duration-200", "motion-reduce:transition-none");
  });

  it.each(["leave", "cancel"])("holds only the bottom box on hover and restarts hiding after %s", (exit) => {
    vi.useFakeTimers();
    render(<LightboxModal selected={selectedAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    act(() => vi.advanceTimersByTime(250));
    const island = screen.getByTestId("lightbox-action-rail");
    const row = island.parentElement!;
    const top = document.querySelector(".lightbox-media-controls");
    fireEvent.pointerEnter(island);
    act(() => vi.advanceTimersByTime(4000));
    expect(row).toHaveStyle({ opacity: "1" });
    expect(row.style.pointerEvents).not.toBe("none");
    expect(top).toHaveStyle({ opacity: "0" });
    // Crossing from a button into metadata never exits the box.
    const favorite = within(island).getByRole("button", { name: "Add to favorites" });
    const metadata = within(island).getByText("IMAGE");
    fireEvent(favorite, new MouseEvent("pointerout", { bubbles: true, relatedTarget: metadata }));
    fireEvent(metadata, new MouseEvent("pointerover", { bubbles: true, relatedTarget: favorite }));
    act(() => vi.advanceTimersByTime(4000));
    expect(row).toHaveStyle({ opacity: "1" });
    if (exit === "leave") fireEvent.pointerLeave(island);
    else fireEvent.pointerCancel(island);
    expect(top).toHaveStyle({ opacity: "1" });
    act(() => vi.advanceTimersByTime(2999));
    expect(row).toHaveStyle({ opacity: "1" });
    act(() => vi.advanceTimersByTime(1));
    expect(row).toHaveStyle({ opacity: "0", pointerEvents: "none" });
    fireEvent(row, new MouseEvent("pointermove", { bubbles: true, clientX: 1, clientY: 1 }));
    fireEvent.pointerEnter(row);
    act(() => vi.advanceTimersByTime(3000));
    expect(row).toHaveStyle({ opacity: "0" });
  });

  it("clears bottom hover when covered by the narrow sidebar", () => {
    vi.useFakeTimers();
    const wideMatchMedia = window.matchMedia;
    Object.defineProperty(window, "matchMedia", { configurable: true,
      value: (query: string) => ({ ...wideMatchMedia(query), matches: query === "(max-width: 767px)" }) });
    render(<LightboxModal selected={selectedAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()} onClose={vi.fn()} />);
    const island = screen.getByTestId("lightbox-action-rail");
    fireEvent.pointerEnter(island);
    fireEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    act(() => vi.advanceTimersByTime(4000));
    expect(island.parentElement).toHaveStyle({ opacity: "0" });
    fireEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    act(() => vi.advanceTimersByTime(250));
    act(() => vi.advanceTimersByTime(3000));
    expect(island.parentElement).toHaveStyle({ opacity: "0" });
  });

  it("retains collapsed state across media navigation and drafts across toggling", async () => {
    const props = {
      selected: selectedAsset, tagEditor: [], onTagEditorChange: vi.fn(), onSaveTags: vi.fn(),
      knownTags: [], onNavigatePrevious: vi.fn(), onNavigateNext: vi.fn(), onToggleFavorite: vi.fn(), onClose: vi.fn()
    };
    const { rerender } = render(<LightboxModal {...props} />);
    const draft = screen.getByPlaceholderText("Type to add tag");
    await userEvent.type(draft, "unfinished");
    await userEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    expect(document.getElementById("lightbox-sidebar")).toHaveAttribute("inert");
    await waitFor(() => expect(screen.getByRole("dialog")).toHaveFocus());
    await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    expect(draft).toHaveValue("unfinished");
    await waitFor(() => expect(screen.getByRole("button", { name: "Close asset panel" })).toHaveFocus());
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("button", { name: "Open asset panel" })).toHaveFocus());
    rerender(<LightboxModal {...props} selected={{ ...selectedAsset, id: 2, kind: "gif" }} />);
    expect(screen.getByRole("button", { name: "Open asset panel" })).toBeInTheDocument();
    expect(document.getElementById("lightbox-sidebar")).toHaveAttribute("inert");
    rerender(<LightboxModal {...props} selected={null} />);
    rerender(<LightboxModal {...props} />);
    expect(screen.getByRole("button", { name: "Close asset panel" })).toBeInTheDocument();
  });

  it("handles left and right arrow navigation", async () => {
    const onNavigatePrevious = vi.fn();
    const onNavigateNext = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={onNavigatePrevious}
        onNavigateNext={onNavigateNext}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    await userEvent.keyboard("{ArrowRight}");
    await userEvent.keyboard("{ArrowLeft}");

    expect(onNavigateNext).toHaveBeenCalledTimes(1);
    expect(onNavigatePrevious).toHaveBeenCalledTimes(1);
  });

  it("toggles picture playback and seeks only with player focus", async () => {
    const onNavigatePrevious = vi.fn();
    const onNavigateNext = vi.fn();
    render(
      <LightboxModal
        selected={selectedVideoAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={onNavigatePrevious}
        onNavigateNext={onNavigateNext}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );
    const player = await waitFor(() => {
      const element = document.querySelector<HTMLElement>("[data-native-video-active]");
      expect(element).not.toBeNull();
      return element!;
    });
    await act(async () => new Promise(requestAnimationFrame));
    await userEvent.click(player);
    expect(player).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}{ArrowRight}");
    await waitFor(() => expect(apiMocks.controlVideo.mock.calls.map((call) => call[1])).toEqual([
      { type: "togglePause" },
      { type: "seekRelative", seconds: -5 },
      { type: "seekRelative", seconds: 5 },
      { type: "seekRelative", seconds: 5 }
    ]));
    expect(onNavigatePrevious).not.toHaveBeenCalled();
    expect(onNavigateNext).not.toHaveBeenCalled();

    apiMocks.controlVideo.mockClear();
    fireEvent.click(player, { button: 2 });
    await userEvent.keyboard("{Control>}{ArrowLeft}{/Control}");
    expect(apiMocks.controlVideo).not.toHaveBeenCalled();
    await userEvent.click(screen.getByLabelText("Add tag"));
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    expect(apiMocks.controlVideo).not.toHaveBeenCalled();
    expect(onNavigateNext).not.toHaveBeenCalled();

    screen.getByRole("dialog").focus();
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    expect(onNavigatePrevious).toHaveBeenCalledOnce();
    expect(onNavigateNext).toHaveBeenCalledOnce();
  });

  it("opens native video by asset id without rendering an HTML video", async () => {
    render(
      <LightboxModal
        selected={selectedVideoAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    await waitFor(() => expect(apiMocks.openVideo).toHaveBeenCalled());
    expect(apiMocks.openVideo.mock.calls[0]?.[0]).toBe(1);
    expect(document.querySelector("video")).toBeNull();
  });

it("gives video a 20px viewport gutter and a single visual frame", () => {
    const { rerender } = render(
      <LightboxModal
        selected={selectedVideoAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const backdrop = document.querySelector("[data-lightbox-backdrop]");
    const dialog = screen.getByRole("dialog");
    const videoStage = document.querySelector('[data-lightbox-media-stage="video"]');

    expect(backdrop).toHaveClass("p-5");
    expect(backdrop).not.toHaveClass("p-2");
    expect(dialog).toHaveAttribute("data-lightbox-kind", "video");
    expect(dialog).toHaveClass(
      "lightbox-shell--video",
      "h-[min(calc(100dvh-40px),1180px)]",
      "w-[min(calc(100vw-40px),1800px)]"
    );
    expect(videoStage).toHaveClass("lightbox-media-stage--video");
    expect(document.querySelector("[data-lightbox-video-player]")).toHaveClass(
      "lightbox-video-player"
    );
    const toolbar = document.querySelector('[data-lightbox-toolbar="video"]');
    expect(toolbar).not.toHaveClass("bottom-[88px]");
    expect(toolbar).toHaveAttribute("aria-label", "Asset actions");
    expect(toolbar?.parentElement).toHaveClass(
      "grid",
      "grid-cols-[minmax(0,1fr)_340px]"
    );

    rerender(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(document.querySelector("[data-lightbox-backdrop]")).toHaveClass("p-2", "sm:p-5");
    expect(screen.getByRole("dialog")).not.toHaveClass("lightbox-shell--video");
    expect(document.querySelector('[data-lightbox-media-stage="image"]')).not.toHaveClass(
      "lightbox-media-stage--video"
    );
  });

  it.each([false, true])("consumes fullscreen Escape and preserves the video session and open sidebar, narrow=%s", async (narrow) => {
    stubMatchMedia(narrow);
    const onClose = vi.fn();
    render(<UiLayerProvider><LightboxModal selected={selectedVideoAsset} tagEditor={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} knownTags={[]}
      onNavigatePrevious={vi.fn()} onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()}
      onClose={onClose} /></UiLayerProvider>);
    await waitFor(() => expect(apiMocks.openVideo).toHaveBeenCalledOnce());
    const eventHandler: (event: MpvVideoEvent) => void = apiMocks.openVideo.mock.calls[0]![4];
    const activeSession = 1;
    if (narrow) await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    const column = document.querySelector(".lightbox-media-column");
    const player = document.querySelector("[data-native-video-active]");
    expect(column).toHaveClass("pt-11");
    act(() => eventHandler({ session_id: activeSession, type: "fullscreen", fullscreen: true }));
    expect(column).not.toHaveClass("pt-11");
    expect(column).not.toHaveAttribute("inert");
    expect(screen.queryByRole("button", { name: "Close preview" })).not.toBeInTheDocument();
    const laterListener = vi.fn();
    window.addEventListener("keydown", laterListener, true);
    const escapeEvent = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    act(() => { player?.dispatchEvent(escapeEvent); });
    window.removeEventListener("keydown", laterListener, true);
    expect(escapeEvent.defaultPrevented).toBe(true);
    expect(laterListener).not.toHaveBeenCalled();
    await waitFor(() => expect(apiMocks.controlVideo).toHaveBeenCalledExactlyOnceWith(activeSession, { type: "toggleFullscreen" }));
    expect(onClose).not.toHaveBeenCalled();
    act(() => eventHandler({ session_id: activeSession, type: "fullscreen", fullscreen: false }));
    expect(column).toHaveClass("pt-11");
    expect(document.getElementById("lightbox-sidebar-close-button")).toHaveAttribute("aria-expanded", "true");
    expect(document.querySelector("[data-native-video-active]")).toBe(player);
    expect(apiMocks.openVideo).toHaveBeenCalledOnce();
    expect(apiMocks.closeVideo).not.toHaveBeenCalled();
  });

  it("shows a video error when native open fails", async () => {
    apiMocks.openVideo.mockRejectedValueOnce(new Error("player unavailable"));

    render(
      <LightboxModal
        selected={selectedVideoAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(await screen.findByRole("alert")).toHaveTextContent("Could not play video");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(document.querySelector("[data-native-video-active]")).not.toBeNull());
    expect(apiMocks.openVideo).toHaveBeenCalledTimes(2);
  });

  it("shows a GIF error and resets it after navigation", () => {
    const gifAsset: SelectedAsset = {
  ...selectedAsset,
  file_name: "animation.gif",
  preview_path: "C:/media/animation.gif",
  path: "C:/media/animation.gif",
  kind: "gif"
};
    const { rerender } = render(
      <LightboxModal
        selected={gifAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    fireEvent.error(screen.getByAltText(gifAsset.file_name));
    expect(screen.getByRole("alert")).toHaveTextContent("Could not display image or GIF");

    rerender(
      <LightboxModal
        selected={{ ...selectedAsset, id: 2, file_name: "2.jpg", path: "C:/media/2.jpg" }}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    expect(screen.getByAltText("2.jpg")).toHaveAttribute("src", "media://C:/media/2.jpg");
  });

  it("does not navigate with arrows while editing tags", async () => {
    const onNavigatePrevious = vi.fn();
    const onNavigateNext = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={onNavigatePrevious}
        onNavigateNext={onNavigateNext}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const tagsInput = screen.getByLabelText("Add tag");
    await userEvent.click(tagsInput);
    tagsInput.focus();
    await userEvent.keyboard("{ArrowRight}");
    await userEvent.keyboard("{ArrowLeft}");

    expect(onNavigateNext).not.toHaveBeenCalled();
    expect(onNavigatePrevious).not.toHaveBeenCalled();
  });

  it("removes tag chip and autosaves", async () => {
    const onTagEditorChange = vi.fn();
    const onSaveTags = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={["cat", "dog"]}
        onTagEditorChange={onTagEditorChange}
        onSaveTags={onSaveTags}
        knownTags={["cat", "dog", "sunset"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(screen.getByTestId("lightbox-tag-list")).toHaveClass(
      "min-h-[48px]",
      "p-2",
      "overflow-auto"
    );
    expect(screen.getByTestId("lightbox-tag-list")).not.toHaveClass("p-1", "w-fit");
    expect(screen.getByTestId("lightbox-tag-list")).toHaveAttribute(
      "data-ui",
      "tag-editor"
    );
    await userEvent.click(screen.getByRole("button", { name: "Remove tag dog" }));

    expect(onTagEditorChange).toHaveBeenCalledWith(["cat"]);
    expect(onSaveTags).toHaveBeenCalledWith(["cat"]);
  });

  it("opens tag suggestions below the input and hides already selected tags", async () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={["cat"]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={["cat", "car", "castle"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const tagsInput = screen.getByPlaceholderText("Type to add tag");
    vi.spyOn(tagsInput, "getBoundingClientRect").mockReturnValue({
      x: 100,
      y: 200,
      top: 200,
      left: 100,
      right: 340,
      bottom: 232,
      width: 240,
      height: 32,
      toJSON: () => ({})
    });
    await userEvent.type(tagsInput, "ca");

    const suggestions = screen.getByRole("listbox", { name: "Tagging suggestions" });
    expect(suggestions).toHaveClass("fixed", "z-[70]");
    expect(suggestions.parentElement).toBe(document.body);
    expect(suggestions).toHaveStyle({ left: "100px", top: "240px", width: "240px" });
    expect(suggestions.style.bottom).toBe("");
    expect(within(suggestions).queryByRole("option", { name: /cat/i })).not.toBeInTheDocument();
    expect(within(suggestions).getByRole("option", { name: /ca\s*r/i })).toBeInTheDocument();
  });

  it("dismisses tag suggestions with Escape without closing the lightbox or losing the draft", async () => {
    const onClose = vi.fn();
    render(<LightboxModal selected={selectedAsset} tagEditor={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} knownTags={["cat"]}
      onNavigatePrevious={vi.fn()} onNavigateNext={vi.fn()}
      onToggleFavorite={vi.fn()} onClose={onClose} />);
    const input = screen.getByLabelText("Add tag");
    await userEvent.type(input, "ca");
    await screen.findByRole("listbox");
    await userEvent.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(input).toHaveValue("ca");
    expect(input).toHaveFocus();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("adds suggested tag with keyboard enter, clears input and keeps tagging panel open", async () => {
    const onTagEditorChange = vi.fn();
    const onSaveTags = vi.fn();

    const { rerender } = render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={["cat"]}
        onTagEditorChange={onTagEditorChange}
        onSaveTags={onSaveTags}
        knownTags={["cat", "car", "castle"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const tagsInput = screen.getByLabelText("Add tag");
    await userEvent.type(tagsInput, "ca");
    await userEvent.keyboard("{ArrowDown}{Enter}");

    expect(tagsInput).toHaveValue("");

    expect(onTagEditorChange).toHaveBeenCalledTimes(1);
    const savedEditor = onTagEditorChange.mock.calls[0]?.[0] as string[];
    expect(savedEditor[0]).toBe("cat");
    expect(savedEditor[1]).toMatch(/^ca/);

    expect(onSaveTags).toHaveBeenCalledTimes(1);
    const savedTags = onSaveTags.mock.calls[0]?.[0] as string[];
    expect(savedTags[0]).toBe("cat");
    expect(savedTags[1]).toMatch(/^ca/);

    rerender(
      <LightboxModal
        selected={{ ...selectedAsset, tags: savedTags }}
        tagEditor={savedTags}
        onTagEditorChange={onTagEditorChange}
        onSaveTags={onSaveTags}
        knownTags={["cat", "car", "castle"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const taggingPanel = screen.getByTestId("lightbox-tag-panel");
    expect(taggingPanel).not.toHaveAttribute("inert");
    expect(taggingPanel).not.toHaveAttribute("aria-hidden", "true");
    expect(screen.getByLabelText("Add tag")).toHaveFocus();
  });

  it("moves highlighted suggestion with arrow up and down", async () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={["cat", "castle", "car"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const tagsInput = screen.getByLabelText("Add tag");
    await userEvent.type(tagsInput, "ca");

    const suggestions = within(screen.getByRole("listbox", { name: "Tagging suggestions" })).getAllByRole("option");
    expect(suggestions[0]).toHaveAttribute("aria-selected", "false");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "false");

    await userEvent.keyboard("{ArrowDown}");
    expect(suggestions[0]).toHaveAttribute("aria-selected", "true");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "false");

    await userEvent.keyboard("{ArrowDown}");
    expect(suggestions[0]).toHaveAttribute("aria-selected", "false");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "true");

    await userEvent.keyboard("{ArrowUp}");
    expect(suggestions[0]).toHaveAttribute("aria-selected", "true");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "false");
  });

  it("focuses tag input so arrow keys work immediately", async () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={["cat", "castle", "car"]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const tagInput = screen.getByLabelText("Add tag");
    await userEvent.click(tagInput);
    await userEvent.type(tagInput, "ca");

    const suggestionsList = await screen.findByRole("listbox", { name: "Tagging suggestions" });
    const suggestions = within(suggestionsList).getAllByRole("option");

    expect(tagInput).toHaveFocus();
    expect(suggestions[0]).toHaveAttribute("aria-selected", "false");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "false");

    await userEvent.keyboard("{ArrowDown}");

    expect(suggestions[0]).toHaveAttribute("aria-selected", "true");
    expect(suggestions[1]).toHaveAttribute("aria-selected", "false");
  });

  it("generates media group uuid and applies media group values", async () => {
    const generatedUuid = "123e4567-e89b-12d3-a456-426614174000";
    const randomUuidSpy = vi.spyOn(globalThis.crypto, "randomUUID").mockReturnValue(generatedUuid);
    const onMediaGroupKeyEditorChange = vi.fn();
    const onSaveMediaGroup = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        mediaGroupKeyEditor="custom-group"
        mediaGroupOrderEditor="0012"
        onMediaGroupKeyEditorChange={onMediaGroupKeyEditorChange}
        onMediaGroupOrderEditorChange={() => {}}
        onSaveMediaGroup={onSaveMediaGroup}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Generate media group key" }));
    expect(onMediaGroupKeyEditorChange).toHaveBeenCalledWith(generatedUuid);

    await userEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onSaveMediaGroup).toHaveBeenCalledWith({ key: "custom-group", order: 12 });

    randomUuidSpy.mockRestore();
  });

  it("toggles favorite from heart button", async () => {
    const onToggleFavorite = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={onToggleFavorite}
        onClose={() => {}}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Add to favorites" }));
    expect(onToggleFavorite).toHaveBeenCalledTimes(1);
  });

  it("shows favorite and media-group save failures with retry guidance", async () => {
    const onToggleFavorite = vi.fn().mockRejectedValue(new Error("favorite failed"));
    const onSaveMediaGroup = vi.fn().mockRejectedValue(new Error("group failed"));

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        mediaGroupKeyEditor="group-a"
        onSaveMediaGroup={onSaveMediaGroup}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={onToggleFavorite}
        onClose={() => {}}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Add to favorites" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The favorite change was not saved. Use the favorite button to retry."
    );

await userEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText(
      "The media group change was not saved. Your values remain available; choose Apply to retry."
    )).toBeInTheDocument();
  });

it("keeps tags inline and info toggleable, off by default", async () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(screen.getByTestId("lightbox-tag-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("lightbox-info-panel")).not.toBeInTheDocument();

    expect(within(screen.getByTestId("lightbox-action-rail")).queryByRole("button", { name: /info/i })).not.toBeInTheDocument();
    expect(document.querySelector(".lightbox-media-column")).not.toHaveTextContent(selectedAsset.file_name);
    expect(screen.getByRole("dialog")).toHaveAccessibleName(/1.jpg/);
    const infoButton = screen.getByRole("button", { name: "Information" });
    await userEvent.click(infoButton);
    expect(screen.getByTestId("lightbox-info-panel")).toBeInTheDocument();
    expect(infoButton).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("lightbox-info-panel")).toHaveTextContent(selectedAsset.file_name);

    await userEvent.click(infoButton);
    expect(screen.queryByTestId("lightbox-info-panel")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Reset zoom" }));
    expect(screen.getByTestId("lightbox-tag-panel")).toBeInTheDocument();
  });

  it("keeps media group above tags and actions in the pinned lower rail", () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const upperSection = screen.getByTestId("lightbox-sidebar-upper");
    const mediaGroup = screen.getByTestId("lightbox-media-group-panel");
    const tags = screen.getByTestId("lightbox-tag-panel");
    const actionRail = screen.getByTestId("lightbox-action-rail");

    expect(upperSection).toContainElement(mediaGroup);
    expect(upperSection).toContainElement(tags);
    expect(mediaGroup.compareDocumentPosition(tags) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(tags).toHaveClass("border-t");
    expect(tags).not.toHaveClass("mt-auto");
    expect(screen.getByRole("button", { name: "Apply" })).toHaveClass("h-8!", "min-h-8!");
    expect(within(actionRail).getAllByRole("button")).toHaveLength(4);
    expect(actionRail.parentElement).not.toHaveClass("overflow-y-auto");
    expect(upperSection).not.toContainElement(actionRail);
  });

  it("keeps actions outside the collapsed panel and opens it for details and deletion", async () => {
    render(<LightboxModal selected={selectedAsset} tagEditor={[]} onTagEditorChange={vi.fn()}
      onSaveTags={vi.fn()} knownTags={[]} onNavigatePrevious={vi.fn()} onNavigateNext={vi.fn()}
      onToggleFavorite={vi.fn()} onClose={vi.fn()} />);
    const panel = document.getElementById("lightbox-sidebar");
    const rail = screen.getByTestId("lightbox-action-rail");
    expect(panel).not.toContainElement(rail);
    await userEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    expect(panel).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("button", { name: "Close preview" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    await userEvent.click(screen.getByRole("button", { name: "Information" }));
    expect(panel).toHaveAttribute("aria-hidden", "false");
    expect(screen.getByTestId("lightbox-info-panel")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close asset panel" }));
    await userEvent.click(document.getElementById("lightbox-delete-button")!);
    expect(panel).toHaveAttribute("aria-hidden", "false");
    await waitFor(() => expect(document.getElementById("lightbox-delete-confirm-input")).toHaveFocus());
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument();
  });

  it("uses a closed drawer on narrow viewports and closes it with Escape", async () => {
    stubMatchMedia(true);
    const onClose = vi.fn();
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={onClose}
      />
    );

    const sidebar = document.querySelector('[data-lightbox-toolbar="image"]');
    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(sidebar).toHaveAttribute("inert");
    expect(sidebar?.parentElement).toHaveClass("grid-rows-[minmax(0,1fr)]");
    expect(screen.getByRole("button", { name: "Open asset panel" })).toHaveAttribute("aria-controls", "lightbox-sidebar");

    await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));

    expect(sidebar).toHaveAttribute("aria-hidden", "false");
    expect(screen.getByTestId("lightbox-sidebar-scrim")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Close asset panel" })).toHaveFocus());

    await userEvent.keyboard("{Escape}");

    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByTestId("lightbox-sidebar-scrim")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Open asset panel" })).toHaveFocus());
    expect(onClose).not.toHaveBeenCalled();
  });

  it("restores keyboard focus only after the narrow video media column returns", async () => {
    const resizeCallbacks: Array<() => void> = [];
    window.ResizeObserver = class {
      constructor(private callback: ResizeObserverCallback) {}
      observe() { resizeCallbacks.push(() => this.callback([], this)); }
      unobserve() {}
      disconnect() {}
    };
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn((query: string) => ({
        matches: query === "(max-width: 767px)",
        addEventListener: vi.fn(), removeEventListener: vi.fn()
      }))
    });
    render(<LightboxModal selected={selectedVideoAsset} tagEditor={[]} knownTags={[]}
      onTagEditorChange={vi.fn()} onSaveTags={vi.fn()} onNavigatePrevious={vi.fn()}
      onNavigateNext={vi.fn()} onToggleFavorite={vi.fn()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    act(() => resizeCallbacks.forEach((callback) => callback()));
    const collapse = await screen.findByRole("button", { name: "Close asset panel" });
    await waitFor(() => expect(collapse).toHaveFocus());
    vi.useFakeTimers();
    fireEvent.click(collapse, { detail: 0 });
    act(() => vi.advanceTimersByTime(100));
    const trigger = screen.getByRole("button", { name: "Open asset panel" });
    expect(trigger.closest(".lightbox-media-column")).toHaveClass("hidden");
    expect(trigger).not.toHaveFocus();
    act(() => vi.advanceTimersByTime(100));
    act(() => vi.advanceTimersByTime(20));
    expect(trigger.closest(".lightbox-media-column")).not.toHaveClass("hidden");
    expect(trigger).toHaveFocus();
  });

  it("hides the native video stage while the narrow drawer is open", async () => {
    const resizeCallbacks: Array<() => void> = [];
    window.ResizeObserver = class {
      constructor(private callback: ResizeObserverCallback) {}
      observe(target: Element) {
        if (target.matches("[data-lightbox-video-player], [data-lightbox-media-stage]")) {
          resizeCallbacks.push(() => this.callback([], this));
        }
      }
      unobserve() {}
      disconnect() {}
    };
    stubMatchMedia(true);
    render(
      <LightboxModal
        selected={selectedVideoAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const videoStage = document.querySelector('[data-lightbox-media-stage="video"]');
    expect(videoStage?.parentElement).not.toHaveClass("hidden");

    await userEvent.click(screen.getByRole("button", { name: "Open asset panel" }));
    expect(videoStage?.parentElement).toHaveClass("hidden");
    act(() => resizeCallbacks.forEach((callback) => callback()));

    await userEvent.click(await screen.findByRole("button", { name: "Close asset panel" }));
    await waitFor(() => expect(document.querySelector('[data-lightbox-media-stage="video"]')?.parentElement).not.toHaveClass("hidden"));
  });

  it("disables tag editing while complete details are loading", async () => {
    const onSaveTags = vi.fn();
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={onSaveTags}
        tagDetailsLoading
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(screen.getByLabelText("Add tag")).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Loading complete tag list");
    expect(onSaveTags).not.toHaveBeenCalled();
  });

  it("shows a details-specific error and delegates its retry separately", async () => {
    const onRetryTagDetails = vi.fn();
    const onRetryTags = vi.fn();
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        onRetryTags={onRetryTags}
        tagDetailsFailed
        assetDetailsFailed
        onRetryTagDetails={onRetryTagDetails}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

expect(screen.getByText("The complete tag list could not be loaded. Editing remains disabled.")).toBeInTheDocument();
    expect(screen.getByText("The media details could not be loaded.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retry loading details" }));
    expect(onRetryTagDetails).toHaveBeenCalledTimes(1);
    expect(onRetryTags).not.toHaveBeenCalled();
  });

it("opens delete confirmation and confirms after typing Yes", async () => {
    const onDeleteMedia = vi.fn();

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onDeleteMedia={onDeleteMedia}
        onClose={() => {}}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Delete media" }));

    const confirmButton = screen.getByRole("button", { name: "Confirm" });
    expect(confirmButton).toBeDisabled();

    await userEvent.type(screen.getByLabelText('Type "Yes"'), "yEs");
    expect(confirmButton).toBeEnabled();

    await userEvent.click(confirmButton);
    expect(onDeleteMedia).toHaveBeenCalledTimes(1);
  });

  it("closes only the inline confirmation on Escape and restores delete focus", async () => {
    const onClose = vi.fn();
    const onNavigateNext = vi.fn();
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={onNavigateNext}
        onToggleFavorite={() => {}}
        onClose={onClose}
      />
    );

    const deleteButton = screen.getByRole("button", { name: "Delete media" });
    await userEvent.click(deleteButton);
    expect(screen.getByTestId("lightbox-delete-confirm-dialog")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(onNavigateNext).not.toHaveBeenCalled();

    const input = screen.getByLabelText('Type "Yes"');
    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByTestId("lightbox-delete-confirm-dialog")).not.toBeInTheDocument()
    );
    expect(deleteButton).toHaveFocus();
    expect(onClose).not.toHaveBeenCalled();
  });
});
