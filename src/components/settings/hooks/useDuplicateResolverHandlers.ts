import { useCallback, type ChangeEvent, type Dispatch, type MouseEvent, type SetStateAction } from "react";
import type { DuplicateResolutionChange } from "../types";

interface UseDuplicateResolverHandlersOptions {
  stagedChanges: DuplicateResolutionChange[];
  stagedChangesByAssetId: Record<number, DuplicateResolutionChange>;
  renameValues: Record<number, string>;
  setRenameValues: Dispatch<SetStateAction<Record<number, string>>>;
  setStagedChangesByAssetId: Dispatch<SetStateAction<Record<number, DuplicateResolutionChange>>>;
  onSaveAll: (changes: DuplicateResolutionChange[]) => Promise<void>;
  createUuidName: (baseFileName: string) => string;
}

function parseAssetId(value: string | undefined): number | null {
  if (!value) {
    return null;
  }

  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return null;
  }

  return parsed;
}

export function useDuplicateResolverHandlers({
  stagedChanges,
  stagedChangesByAssetId,
  renameValues,
  setRenameValues,
  setStagedChangesByAssetId,
  onSaveAll,
  createUuidName
}: UseDuplicateResolverHandlersOptions) {
  const handleSaveAllClick = useCallback(async () => {
    await onSaveAll(stagedChanges);
  }, [onSaveAll, stagedChanges]);

  const handleRenameValueChange = useCallback(
    (event: ChangeEvent<HTMLInputElement>) => {
      const assetId = parseAssetId(event.currentTarget.dataset.assetId);
      if (!assetId) {
        return;
      }

      const nextValue = event.currentTarget.value;
      setRenameValues((previous) => ({
        ...previous,
        [assetId]: nextValue
      }));

      if (stagedChangesByAssetId[assetId]?.type === "rename") {
        setStagedChangesByAssetId((previous) => ({
          ...previous,
          [assetId]: {
            assetId,
            type: "rename",
            nextFileName: nextValue.trim()
          }
        }));
      }
    },
    [setRenameValues, setStagedChangesByAssetId, stagedChangesByAssetId]
  );

  const handleGenerateUuidClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const assetId = parseAssetId(event.currentTarget.dataset.assetId);
      const currentName = event.currentTarget.dataset.currentName;
      if (!assetId || !currentName) {
        return;
      }

      setRenameValues((previous) => ({
        ...previous,
        [assetId]: createUuidName(currentName)
      }));
    },
    [createUuidName, setRenameValues]
  );

  const handleQueueRenameClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const assetId = parseAssetId(event.currentTarget.dataset.assetId);
      if (!assetId) {
        return;
      }

      const nextName = renameValues[assetId]?.trim() ?? "";
      if (!nextName) {
        return;
      }

      setStagedChangesByAssetId((previous) => ({
        ...previous,
        [assetId]: {
          assetId,
          type: "rename",
          nextFileName: nextName
        }
      }));
    },
    [renameValues, setStagedChangesByAssetId]
  );

  const handleToggleDeleteClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const assetId = parseAssetId(event.currentTarget.dataset.assetId);
      if (!assetId) {
        return;
      }

      setStagedChangesByAssetId((previous) => {
        if (previous[assetId]?.type === "delete") {
          const next = { ...previous };
          delete next[assetId];
          return next;
        }

        return {
          ...previous,
          [assetId]: {
            assetId,
            type: "delete"
          }
        };
      });
    },
    [setStagedChangesByAssetId]
  );

  const handleClearActionClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const assetId = parseAssetId(event.currentTarget.dataset.assetId);
      if (!assetId) {
        return;
      }

      setStagedChangesByAssetId((previous) => {
        const next = { ...previous };
        delete next[assetId];
        return next;
      });
    },
    [setStagedChangesByAssetId]
  );

  return {
    handleSaveAllClick,
    handleRenameValueChange,
    handleGenerateUuidClick,
    handleQueueRenameClick,
    handleToggleDeleteClick,
    handleClearActionClick
  };
}
