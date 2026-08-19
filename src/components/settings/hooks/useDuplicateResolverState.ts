import { useEffect, useMemo, useState } from "react";
import {
  getFileNameFromPath,
  validateDuplicateResolutionChanges
} from "../services/duplicateValidationService";
import type { DuplicateGroup } from "../../../types";
import type { DuplicateResolutionChange } from "../types";

function buildInitialRenameValues(groups: DuplicateGroup[]): Record<number, string> {
  const nextValues: Record<number, string> = {};
  for (const group of groups) {
    for (const asset of group.assets) {
      nextValues[asset.id] = getFileNameFromPath(asset.path);
    }
  }
  return nextValues;
}

export function useDuplicateResolverState(open: boolean, groups: DuplicateGroup[]) {
  const [renameValues, setRenameValues] = useState<Record<number, string>>({});
  const [stagedChangesByAssetId, setStagedChangesByAssetId] = useState<
    Record<number, DuplicateResolutionChange>
  >({});

  const resetForm = () => {
    setRenameValues(buildInitialRenameValues(groups));
    setStagedChangesByAssetId({});
  };

  useEffect(() => {
    if (!open) {
      return;
    }

    resetForm();
  }, [groups, open]);

  const duplicateAssetCount = useMemo(() => {
    return groups.reduce((total, group) => total + group.assets.length, 0);
  }, [groups]);

  const stagedChanges = useMemo(() => {
    return Object.values(stagedChangesByAssetId);
  }, [stagedChangesByAssetId]);

  const validationResult = useMemo(
    () => validateDuplicateResolutionChanges(groups, stagedChanges),
    [groups, stagedChanges]
  );

  return {
    renameValues,
    setRenameValues,
    stagedChangesByAssetId,
    setStagedChangesByAssetId,
    duplicateAssetCount,
    stagedChanges,
    validationResult,
    resetForm
  };
}
