import { act, renderHook } from "@testing-library/react";
import type { MouseEvent, WheelEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../../types";
import { useGalleryGridHandlers } from "../useGalleryGridHandlers";

function createAsset(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  }
}

function createTileElement(assetId: number, assetIndex: number): HTMLButtonElement {
  const tile = document.createElement("button");
  tile.dataset.assetId = String(assetId);
  tile.dataset.assetIndex = String(assetIndex);
  return tile;
}

describe("useGalleryGridHandlers", () => {
  it("handles ctrl wheel zoom", () => {
    const onCtrlWheelZoom = vi.fn();

    const { result } = renderHook(() =>
      useGalleryGridHandlers({
        assets: [createAsset(1)],
        selectionModeEnabled: false,
        onCtrlWheelZoom,
        onSelect: vi.fn()
      })
    );

    const wheelEvent = {
      ctrlKey: true,
      deltaY: -120,
      preventDefault: vi.fn()
    } as unknown as WheelEvent<HTMLElement>;

    act(() => {
      result.current.handleGalleryWheel(wheelEvent);
    });

    expect(wheelEvent.preventDefault).toHaveBeenCalledTimes(1);
    expect(onCtrlWheelZoom).toHaveBeenCalledWith(-120);
  });

  it("uses lightbox selection when selection mode is disabled", () => {
    const asset = createAsset(7);
    const onSelect = vi.fn();
    const onBulkSelectionInteraction = vi.fn();

    const { result } = renderHook(() =>
      useGalleryGridHandlers({
        assets: [asset],
        selectionModeEnabled: false,
        onSelect,
        onBulkSelectionInteraction
      })
    );

    const clickEvent = {
      currentTarget: createTileElement(asset.id, 0),
      ctrlKey: false,
      metaKey: false,
      shiftKey: false,
      preventDefault: vi.fn()
    } as unknown as MouseEvent<HTMLButtonElement>;

    act(() => {
      result.current.handleTileClick(clickEvent);
    });

    expect(onSelect).toHaveBeenCalledWith(asset);
    expect(onBulkSelectionInteraction).not.toHaveBeenCalled();
    expect(clickEvent.preventDefault).not.toHaveBeenCalled();
  });

  it("supports drag and ctrl click interactions in selection mode", () => {
    const first = createAsset(1);
    const second = createAsset(2);
    const onBulkSelectionInteraction = vi.fn();

    const { result } = renderHook(() =>
      useGalleryGridHandlers({
        assets: [first, second],
        selectionModeEnabled: true,
        onSelect: vi.fn(),
        onBulkSelectionInteraction
      })
    );

    const mouseDownEvent = {
      currentTarget: createTileElement(first.id, 0),
      button: 0,
      ctrlKey: false,
      metaKey: false,
      shiftKey: false,
      preventDefault: vi.fn()
    } as unknown as MouseEvent<HTMLButtonElement>;
    const mouseEnterEvent = {
      currentTarget: createTileElement(second.id, 1)
    } as unknown as MouseEvent<HTMLButtonElement>;

    act(() => {
      result.current.handleTileMouseDown(mouseDownEvent);
      result.current.handleTileMouseEnter(mouseEnterEvent);
    });

    expect(result.current.isDragSelecting).toBe(true);
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(1, {
      assetId: 1,
      assetIndex: 0,
      ctrlLike: false,
      shift: false,
      viaDrag: true
    });
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(2, {
      assetId: 2,
      assetIndex: 1,
      ctrlLike: false,
      shift: false,
      viaDrag: true
    });

    const ctrlClickEvent = {
      currentTarget: createTileElement(second.id, 1),
      ctrlKey: true,
      metaKey: false,
      shiftKey: false,
      preventDefault: vi.fn()
    } as unknown as MouseEvent<HTMLButtonElement>;

    act(() => {
      result.current.handleTileClick(ctrlClickEvent);
    });

    expect(ctrlClickEvent.preventDefault).toHaveBeenCalledTimes(1);
    expect(onBulkSelectionInteraction).toHaveBeenNthCalledWith(3, {
      assetId: 2,
      assetIndex: 1,
      ctrlLike: true,
      shift: false,
      viaDrag: false
    });
    expect(result.current.isDragSelecting).toBe(false);
  });
});
