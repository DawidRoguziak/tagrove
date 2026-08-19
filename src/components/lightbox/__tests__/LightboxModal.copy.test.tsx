import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset } from "../../../types";
import { LightboxModal } from "../LightboxModal";

vi.mock("../../../api", () => ({
  toMediaSrc: (path: string) => `media://${path}`
}));

const selectedAsset: Asset = {
  id: 1,
  path: "C:/media/1.jpg",
  kind: "image",
  size_bytes: 1024,
  modified_at: 1700000000,
  width: 1200,
  height: 800,
  duration_ms: null,
  thumb_path: null,
  is_favorite: false,
  media_group_key: null,
  media_group_order: null,
  tags: []
};

describe("LightboxModal copy media group", () => {
  const originalClipboard = navigator.clipboard;

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllTimers();
    vi.useRealTimers();

    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: originalClipboard
    });
  });

  it("disables copy button when media group name is empty", () => {
    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        mediaGroupKeyEditor="   "
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    expect(screen.getByRole("button", { name: "No group name to copy" })).toBeDisabled();
  });

  it("copies media group name to clipboard and clears copied state after timeout", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText }
    });

    render(
      <LightboxModal
        selected={selectedAsset}
        tagEditor={[]}
        onTagEditorChange={() => {}}
        onSaveTags={() => {}}
        mediaGroupKeyEditor="group-alpha"
        knownTags={[]}
        onNavigatePrevious={() => {}}
        onNavigateNext={() => {}}
        onToggleFavorite={() => {}}
        onClose={() => {}}
      />
    );

    const copyButton = screen.getByRole("button", { name: "Copy group name" });

    await act(async () => {
      fireEvent.click(copyButton);
      await Promise.resolve();
    });

    expect(writeText).toHaveBeenCalledWith("group-alpha");
    expect(screen.getByRole("button", { name: "Group name copied" })).toBeEnabled();

    await act(async () => {
      vi.advanceTimersByTime(1600);
      await Promise.resolve();
    });

    expect(screen.getByRole("button", { name: "Copy group name" })).toBeEnabled();
  });
});
