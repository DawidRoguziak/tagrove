import { useCallback, useRef, type MutableRefObject } from "react";
import type { AssetSummary } from "../types";
interface Options {
  assetCount: number;
  getAssetAtAsync: (index: number) => Promise<AssetSummary | undefined>;
  selectAsset: (asset: AssetSummary, index: number) => void;
  selectedIndexRef: MutableRefObject<number | null>;
  navigationTargetIndexRef: MutableRefObject<number | null>;
  navigationRequestRef: MutableRefObject<number>;
}
/** Rapid key presses advance the pending global index; stale reads cannot navigate. */
export function useSelectionNavigation({
  assetCount,
  getAssetAtAsync,
  selectAsset,
  selectedIndexRef,
  navigationTargetIndexRef,
  navigationRequestRef
}: Options) {
  const selectRef = useRef(selectAsset);
  selectRef.current = selectAsset;
  const navigateBy = useCallback(
    (step: -1 | 1) => {
      const currentIndex = navigationTargetIndexRef.current ?? selectedIndexRef.current;
      if (currentIndex === null || assetCount === 0) return;
      const targetIndex = (currentIndex + step + assetCount) % assetCount;
      navigationTargetIndexRef.current = targetIndex;
      const requestId = ++navigationRequestRef.current;
      void getAssetAtAsync(targetIndex)
        .then((asset) => {
          if (navigationRequestRef.current !== requestId) return;
          if (!asset) {
            navigationTargetIndexRef.current = currentIndex;
            return;
          }
          selectRef.current(asset, targetIndex);
        })
        .catch(() => {
          if (navigationRequestRef.current === requestId)
            navigationTargetIndexRef.current = currentIndex;
        });
    },
    [assetCount, getAssetAtAsync, navigationRequestRef, navigationTargetIndexRef, selectedIndexRef]
  );
  return {
    handleSelectPrevious: useCallback(() => navigateBy(-1), [navigateBy]),
    handleSelectNext: useCallback(() => navigateBy(1), [navigateBy])
  };
}
