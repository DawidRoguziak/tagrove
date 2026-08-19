import { act, renderHook } from "@testing-library/react";
import type { MouseEvent, PointerEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import { useLightboxModalHandlers } from "../useLightboxModalHandlers";

describe("useLightboxModalHandlers", () => {
  it("toggles popovers and resets state on selected id change", () => {
    const tagContainer = document.createElement("div");
    const infoContainer = document.createElement("div");

    const { result, rerender } = renderHook(
      ({ selectedId }: { selectedId: number | null }) =>
        useLightboxModalHandlers({
          selectedId,
          mediaGroupKeyEditor: "group-a",
          mediaGroupOrderEditor: "2",
          onSaveMediaGroup: vi.fn(),
          onDeleteMedia: vi.fn(async () => {}),
          onClose: vi.fn(),
          tagPopoverContainerRef: { current: tagContainer },
          infoPopoverContainerRef: { current: infoContainer }
        }),
      { initialProps: { selectedId: 1 } }
    );

    act(() => {
      result.current.handleToggleTagsPanel();
    });
    expect(result.current.tagsPanelOpen).toBe(true);
    expect(result.current.infoPanelOpen).toBe(false);

    act(() => {
      result.current.handleToggleInfoPanel();
    });
    expect(result.current.infoPanelOpen).toBe(true);
    expect(result.current.tagsPanelOpen).toBe(false);

    act(() => {
      result.current.handleOpenDeleteConfirm();
    });
    expect(result.current.deleteConfirmOpen).toBe(true);
    expect(result.current.tagsPanelOpen).toBe(false);
    expect(result.current.infoPanelOpen).toBe(false);

    rerender({ selectedId: 2 });

    expect(result.current.tagsPanelOpen).toBe(false);
    expect(result.current.infoPanelOpen).toBe(false);
    expect(result.current.deleteConfirmOpen).toBe(false);
    expect(result.current.deleteSubmitting).toBe(false);
  });

  it("applies media group values with null and numeric order", () => {
    const onSaveMediaGroup = vi.fn();

    const { result, rerender } = renderHook(
      ({ order }: { order: string }) =>
        useLightboxModalHandlers({
          selectedId: 1,
          mediaGroupKeyEditor: " group-key ",
          mediaGroupOrderEditor: order,
          onSaveMediaGroup,
          onDeleteMedia: vi.fn(async () => {}),
          onClose: vi.fn(),
          tagPopoverContainerRef: { current: document.createElement("div") },
          infoPopoverContainerRef: { current: document.createElement("div") }
        }),
      { initialProps: { order: "12.5" } }
    );

    act(() => {
      result.current.handleApplyMediaGroup();
    });
    expect(onSaveMediaGroup).toHaveBeenCalledWith({ key: "group-key", order: 12.5 });

    rerender({ order: "" });
    act(() => {
      result.current.handleApplyMediaGroup();
    });
    expect(onSaveMediaGroup).toHaveBeenCalledWith({ key: "group-key", order: null });

    rerender({ order: "not-a-number" });
    act(() => {
      result.current.handleApplyMediaGroup();
    });
    expect(onSaveMediaGroup).toHaveBeenCalledTimes(2);
  });

  it("closes tag popover when pointer event target is outside", () => {
    const tagContainer = document.createElement("div");
    const infoContainer = document.createElement("div");
    const insideTag = document.createElement("button");
    tagContainer.appendChild(insideTag);
    const outside = document.createElement("div");

    const { result } = renderHook(() =>
      useLightboxModalHandlers({
        selectedId: 1,
        mediaGroupKeyEditor: "group",
        mediaGroupOrderEditor: "1",
        onSaveMediaGroup: vi.fn(),
        onDeleteMedia: vi.fn(async () => {}),
        onClose: vi.fn(),
        tagPopoverContainerRef: { current: tagContainer },
        infoPopoverContainerRef: { current: infoContainer }
      })
    );

    act(() => {
      result.current.handleToggleTagsPanel();
    });
    expect(result.current.tagsPanelOpen).toBe(true);

    const insideEvent = {
      target: insideTag
    } as unknown as PointerEvent<HTMLDivElement>;
    act(() => {
      result.current.handleShellPointerDownCapture(insideEvent);
    });
    expect(result.current.tagsPanelOpen).toBe(true);

    const outsideEvent = {
      target: outside
    } as unknown as PointerEvent<HTMLDivElement>;
    act(() => {
      result.current.handleShellPointerDownCapture(outsideEvent);
    });
    expect(result.current.tagsPanelOpen).toBe(false);
  });

  it("handles delete confirmation flow and blocks close while submitting", async () => {
    const onClose = vi.fn();
    let resolveDelete: (() => void) | null = null;
    const onDeleteMedia = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveDelete = resolve;
        })
    );

    const { result } = renderHook(() =>
      useLightboxModalHandlers({
        selectedId: 1,
        mediaGroupKeyEditor: "group",
        mediaGroupOrderEditor: "1",
        onSaveMediaGroup: vi.fn(),
        onDeleteMedia,
        onClose,
        tagPopoverContainerRef: { current: document.createElement("div") },
        infoPopoverContainerRef: { current: document.createElement("div") }
      })
    );

    act(() => {
      result.current.handleOpenDeleteConfirm();
      result.current.handleConfirmDeleteMedia();
    });

    expect(onDeleteMedia).toHaveBeenCalledTimes(1);
    expect(result.current.deleteSubmitting).toBe(true);

    act(() => {
      result.current.handleCloseDeleteConfirm();
    });
    expect(result.current.deleteConfirmOpen).toBe(true);

    await act(async () => {
      resolveDelete?.();
      await Promise.resolve();
    });

    expect(result.current.deleteSubmitting).toBe(false);
    expect(result.current.deleteConfirmOpen).toBe(false);

    const shellClickEvent = {
      stopPropagation: vi.fn()
    } as unknown as MouseEvent<HTMLDivElement>;

    act(() => {
      result.current.handleShellClick(shellClickEvent);
      result.current.handleCloseLightbox();
    });

    expect(shellClickEvent.stopPropagation).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
