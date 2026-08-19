import { act, renderHook } from "@testing-library/react";
import type { ChangeEvent, MouseEvent } from "react";
import { describe, expect, it, vi } from "vitest";
import type { DuplicateResolutionChange } from "../../types";
import { useDuplicateResolverHandlers } from "../useDuplicateResolverHandlers";

describe("useDuplicateResolverHandlers", () => {
  it("saves staged changes and updates rename draft", async () => {
    const stagedChanges: DuplicateResolutionChange[] = [
      {
        assetId: 1,
        type: "rename",
        nextFileName: "a.jpg"
      }
    ];
    const onSaveAll = vi.fn(async () => {});
    const setRenameValues = vi.fn();
    const setStagedChangesByAssetId = vi.fn();

    const { result } = renderHook(() =>
      useDuplicateResolverHandlers({
        stagedChanges,
        stagedChangesByAssetId: {
          1: {
            assetId: 1,
            type: "rename",
            nextFileName: "a.jpg"
          }
        },
        renameValues: { 1: "a.jpg" },
        setRenameValues,
        setStagedChangesByAssetId,
        onSaveAll,
        createUuidName: () => "uuid-name.jpg"
      })
    );

    await act(async () => {
      await result.current.handleSaveAllClick();
    });
    expect(onSaveAll).toHaveBeenCalledWith(stagedChanges);

    const changeEvent = {
      currentTarget: {
        dataset: { assetId: "1" },
        value: " next-name.png "
      }
    } as unknown as ChangeEvent<HTMLInputElement>;

    act(() => {
      result.current.handleRenameValueChange(changeEvent);
    });

    const renameUpdater = setRenameValues.mock.calls[0]?.[0] as (
      previous: Record<number, string>
    ) => Record<number, string>;
    expect(renameUpdater({})).toEqual({ 1: " next-name.png " });

    const stagedUpdater = setStagedChangesByAssetId.mock.calls[0]?.[0] as (
      previous: Record<number, DuplicateResolutionChange>
    ) => Record<number, DuplicateResolutionChange>;
    expect(stagedUpdater({})).toEqual({
      1: {
        assetId: 1,
        type: "rename",
        nextFileName: "next-name.png"
      }
    });
  });

  it("queues rename and toggles delete actions by asset id", () => {
    const setStagedChangesByAssetId = vi.fn();

    const { result } = renderHook(() =>
      useDuplicateResolverHandlers({
        stagedChanges: [],
        stagedChangesByAssetId: {},
        renameValues: { 7: " queued.jpg " },
        setRenameValues: vi.fn(),
        setStagedChangesByAssetId,
        onSaveAll: vi.fn(async () => {}),
        createUuidName: () => "uuid-name.jpg"
      })
    );

    const queueRenameEvent = {
      currentTarget: {
        dataset: { assetId: "7" }
      }
    } as unknown as MouseEvent<HTMLButtonElement>;

    act(() => {
      result.current.handleQueueRenameClick(queueRenameEvent);
    });

    const renameUpdater = setStagedChangesByAssetId.mock.calls[0]?.[0] as (
      previous: Record<number, DuplicateResolutionChange>
    ) => Record<number, DuplicateResolutionChange>;
    expect(renameUpdater({})).toEqual({
      7: {
        assetId: 7,
        type: "rename",
        nextFileName: "queued.jpg"
      }
    });

    const toggleDeleteEvent = {
      currentTarget: {
        dataset: { assetId: "7" }
      }
    } as unknown as MouseEvent<HTMLButtonElement>;

    act(() => {
      result.current.handleToggleDeleteClick(toggleDeleteEvent);
      result.current.handleToggleDeleteClick(toggleDeleteEvent);
    });

    const addDeleteUpdater = setStagedChangesByAssetId.mock.calls[1]?.[0] as (
      previous: Record<number, DuplicateResolutionChange>
    ) => Record<number, DuplicateResolutionChange>;
    expect(addDeleteUpdater({})).toEqual({
      7: {
        assetId: 7,
        type: "delete"
      }
    });

    const removeDeleteUpdater = setStagedChangesByAssetId.mock.calls[2]?.[0] as (
      previous: Record<number, DuplicateResolutionChange>
    ) => Record<number, DuplicateResolutionChange>;
    expect(removeDeleteUpdater({ 7: { assetId: 7, type: "delete" } })).toEqual({});
  });
});
