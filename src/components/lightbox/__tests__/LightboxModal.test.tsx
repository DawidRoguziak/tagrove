import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../types";
import { LightboxModal } from "../LightboxModal";

const apiMocks = vi.hoisted(() => ({
  openVideo: vi.fn(),
  setVideoBounds: vi.fn(),
  controlVideo: vi.fn(),
  closeVideo: vi.fn()
}));

vi.mock("../../../api", () => ({
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
  beforeEach(() => {
    stubMatchMedia(false);
    apiMocks.openVideo.mockReset().mockResolvedValue(1);
    apiMocks.setVideoBounds.mockReset().mockResolvedValue(undefined);
    apiMocks.controlVideo.mockReset().mockResolvedValue(undefined);
    apiMocks.closeVideo.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    window.ResizeObserver = originalResizeObserver;
    Object.defineProperty(window, "matchMedia", { configurable: true, value: originalMatchMedia });
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
      "grid-cols-[minmax(0,1fr)_clamp(18rem,22vw,22rem)]"
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

  it("uses an immersive shell and hides the lightbox toolbar in video fullscreen", async () => {
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
    const activeSession = apiMocks.setVideoBounds.mock.calls.at(-1)?.[0] as number;
    const eventHandler = apiMocks.openVideo.mock.calls.at(-1)?.[4] as
      | ((event: { session_id: number; type: "fullscreen"; fullscreen: boolean }) => void)
      | undefined;

    expect(screen.getByRole("button", { name: "Close preview" })).toBeInTheDocument();
    act(() => {
      eventHandler?.({ session_id: activeSession, type: "fullscreen", fullscreen: true });
    });

    expect(screen.getByRole("dialog")).toHaveClass(
      "h-full",
      "w-full",
      "rounded-none",
      "border-0"
    );
    expect(screen.queryByRole("button", { name: "Close preview" })).not.toBeInTheDocument();
    expect(document.querySelector("[data-native-video-active]")).not.toBeNull();
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
      "px-0",
      "py-2",
      "overflow-auto"
    );
    expect(screen.getByTestId("lightbox-tag-list")).not.toHaveClass("p-1", "w-fit");
    expect(screen.getByTestId("lightbox-tag-list")).toHaveAttribute(
      "data-ui",
      "assigned-tag-list"
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
        mediaGroupOrderEditor="12.5"
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
    expect(onSaveMediaGroup).toHaveBeenCalledWith({ key: "custom-group", order: 12.5 });

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

    const infoButton = screen.getByRole("button", { name: "Show info" });
    await userEvent.click(infoButton);
    expect(screen.getByTestId("lightbox-info-panel")).toBeInTheDocument();
    expect(infoButton).toHaveAttribute("aria-expanded", "true");

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
    expect(screen.getByRole("button", { name: "Apply" })).toHaveClass("h-6!", "min-h-6!");
    expect(within(actionRail).getAllByRole("button")).toHaveLength(6);
    expect(upperSection).not.toContainElement(actionRail);
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
