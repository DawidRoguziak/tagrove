import { act, renderHook } from "@testing-library/react";
import type { MouseEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import { useLightboxModalHandlers } from "../useLightboxModalHandlers";

describe("useLightboxModalHandlers", () => {
  it("opens the sidebar, toggles info and resets state on selected id change", () => {
    const { result, rerender } = renderHook(
      ({ selectedId }: { selectedId: number | null }) =>
        useLightboxModalHandlers({
          selectedId,
          mediaGroupKeyEditor: "group-a",
          mediaGroupOrderEditor: "2",
          onSaveMediaGroup: vi.fn(),
          onDeleteMedia: vi.fn(async () => {}),
          onClose: vi.fn()
        }),
      { initialProps: { selectedId: 1 } }
    );

    act(() => {
      result.current.handleOpenSidebar();
    });
    expect(result.current.sidebarOpen).toBe(true);
    expect(result.current.infoPanelOpen).toBe(false);

    act(() => {
      result.current.handleToggleInfoPanel();
    });
    expect(result.current.infoPanelOpen).toBe(true);
    expect(result.current.sidebarOpen).toBe(true);

    act(() => {
      result.current.handleOpenDeleteConfirm();
    });
    expect(result.current.deleteConfirmOpen).toBe(true);
    expect(result.current.sidebarOpen).toBe(true);
    expect(result.current.infoPanelOpen).toBe(false);

    rerender({ selectedId: 2 });

    expect(result.current.sidebarOpen).toBe(false);
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
          onClose: vi.fn()
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
        onClose
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
