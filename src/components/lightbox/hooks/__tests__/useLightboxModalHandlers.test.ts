import { act, renderHook } from "@testing-library/react";
import type { MouseEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import { useLightboxModalHandlers } from "../useLightboxModalHandlers";

describe("useLightboxModalHandlers", () => {
  it("toggles info and resets selection-scoped state on selected id change", () => {
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

    expect(result.current.infoPanelOpen).toBe(false);

    act(() => {
      result.current.handleToggleInfoPanel();
    });
    expect(result.current.infoPanelOpen).toBe(true);

    act(() => {
      result.current.handleOpenDeleteConfirm();
    });
    expect(result.current.deleteConfirmOpen).toBe(true);
    expect(result.current.infoPanelOpen).toBe(false);

    rerender({ selectedId: 2 });

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
      { initialProps: { order: "0012" } }
    );

    act(() => {
      result.current.handleApplyMediaGroup();
    });
    expect(onSaveMediaGroup).toHaveBeenCalledWith({ key: "group-key", order: 12 });

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

  it.each(["0", "00", "-1", "+1", "1.5", "1e3", "9007199254740992", " 2", "2 ", "2\n", "abc", "Infinity"])("guards direct submission of invalid draft %j", (order) => {
    const onSaveMediaGroup = vi.fn();
    const { result } = renderHook(() => useLightboxModalHandlers({
      selectedId: 1,
      mediaGroupKeyEditor: "group",
      mediaGroupOrderEditor: order,
      onSaveMediaGroup,
      onDeleteMedia: vi.fn(),
      onClose: vi.fn()
    }));
    act(() => result.current.handleApplyMediaGroup());
    expect(onSaveMediaGroup).not.toHaveBeenCalled();
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
