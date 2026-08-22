import { useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { AssetSummary } from "../types";

interface UseLibraryLifecycleOptions {
  resetThumbnailQueue: () => void;
  setThumbs: Dispatch<SetStateAction<Record<number, string>>>;
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  setTotal: Dispatch<SetStateAction<number>>;
  setOffset: Dispatch<SetStateAction<number>>;
  setKnownTags: Dispatch<SetStateAction<string[]>>;
}

export function useLibraryLifecycle({
  resetThumbnailQueue,
  setThumbs,
  setAssets,
  setTotal,
  setOffset,
  setKnownTags
}: UseLibraryLifecycleOptions) {
  const handleRootRemoved = useCallback(() => {
    resetThumbnailQueue();
  }, [resetThumbnailQueue]);

  const handleImportDbRestored = useCallback(() => {
    resetThumbnailQueue();
    setThumbs({});
  }, [resetThumbnailQueue, setThumbs]);

  const handleLibraryCleared = useCallback(() => {
    resetThumbnailQueue();
    setThumbs({});
    setAssets([]);
    setTotal(0);
    setOffset(0);
    setKnownTags([]);
  }, [resetThumbnailQueue, setAssets, setKnownTags, setOffset, setThumbs, setTotal]);

  return {
    handleRootRemoved,
    handleImportDbRestored,
    handleLibraryCleared
  };
}
