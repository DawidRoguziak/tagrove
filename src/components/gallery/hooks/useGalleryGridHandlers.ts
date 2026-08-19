import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type WheelEvent } from "react";
import type { Asset } from "../../../types";
import type { BulkSelectionInteraction } from "../GalleryGrid";

interface UseGalleryGridHandlersOptions {
  assets: Asset[];
  getAssetAt?: (index: number) => Asset | undefined;
  selectionModeEnabled: boolean;
  onCtrlWheelZoom?: (deltaY: number) => void;
  onSelect: (asset: Asset) => void;
  onBulkSelectionInteraction?: (interaction: BulkSelectionInteraction) => void;
}

interface TileContext {
  asset: Asset;
  assetIndex: number;
}

function parsePositiveInteger(value: string | undefined): number | null {
  if (!value) {
    return null;
  }

  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 0) {
    return null;
  }

  return parsed;
}

export function useGalleryGridHandlers({
  assets,
  getAssetAt = (index) => assets[index],
  selectionModeEnabled,
  onCtrlWheelZoom,
  onSelect,
  onBulkSelectionInteraction
}: UseGalleryGridHandlersOptions) {
  const [isDragSelecting, setIsDragSelecting] = useState(false);
  const dragSelectingRef = useRef(false);
  const dragMovedRef = useRef(false);

  const assetById = useMemo(() => {
    const lookup = new Map<number, Asset>();
    for (const asset of assets) {
      lookup.set(asset.id, asset);
    }
    return lookup;
  }, [assets]);

  const stopDragSelecting = useCallback(() => {
    if (!dragSelectingRef.current) {
      return;
    }

    dragSelectingRef.current = false;
    setIsDragSelecting(false);
  }, []);

  useEffect(() => {
    if (!selectionModeEnabled) {
      stopDragSelecting();
      return;
    }

    const handleMouseUp = () => {
      stopDragSelecting();
    };

    window.addEventListener("mouseup", handleMouseUp);
    window.addEventListener("blur", handleMouseUp);
    return () => {
      window.removeEventListener("mouseup", handleMouseUp);
      window.removeEventListener("blur", handleMouseUp);
    };
  }, [selectionModeEnabled, stopDragSelecting]);

  const readTileContext = useCallback(
    (currentTarget: HTMLButtonElement): TileContext | null => {
      const assetId = parsePositiveInteger(currentTarget.dataset.assetId);
      const assetIndex = parsePositiveInteger(currentTarget.dataset.assetIndex);
      if (assetId === null || assetIndex === null) {
        return null;
      }

      const indexedAsset = getAssetAt(assetIndex);
      if (indexedAsset && indexedAsset.id === assetId) {
        return {
          asset: indexedAsset,
          assetIndex
        };
      }

      const mappedAsset = assetById.get(assetId);
      if (!mappedAsset) {
        return null;
      }

      return {
        asset: mappedAsset,
        assetIndex
      };
    },
    [assetById, getAssetAt]
  );

  const handleGalleryWheel = useCallback(
    (event: WheelEvent<HTMLElement>) => {
      if (!event.ctrlKey) {
        return;
      }

      event.preventDefault();
      onCtrlWheelZoom?.(event.deltaY);
    },
    [onCtrlWheelZoom]
  );

  const handleTileMouseDown = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      if (!selectionModeEnabled || event.button !== 0) {
        return;
      }

      const context = readTileContext(event.currentTarget);
      if (!context) {
        return;
      }

      const ctrlLike = event.ctrlKey || event.metaKey;
      if (ctrlLike || event.shiftKey) {
        return;
      }

      onBulkSelectionInteraction?.({
        assetId: context.asset.id,
        assetIndex: context.assetIndex,
        ctrlLike: false,
        shift: false,
        viaDrag: true
      });
      dragMovedRef.current = false;
      dragSelectingRef.current = true;
      setIsDragSelecting(true);
      event.preventDefault();
    },
    [onBulkSelectionInteraction, readTileContext, selectionModeEnabled]
  );

  const handleTileMouseEnter = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      if (!selectionModeEnabled || !dragSelectingRef.current) {
        return;
      }

      const context = readTileContext(event.currentTarget);
      if (!context) {
        return;
      }

      dragMovedRef.current = true;
      onBulkSelectionInteraction?.({
        assetId: context.asset.id,
        assetIndex: context.assetIndex,
        ctrlLike: false,
        shift: false,
        viaDrag: true
      });
    },
    [onBulkSelectionInteraction, readTileContext, selectionModeEnabled]
  );

  const handleTileClick = useCallback(
    (event: MouseEvent<HTMLButtonElement>) => {
      const context = readTileContext(event.currentTarget);
      if (!context) {
        return;
      }

      if (!selectionModeEnabled) {
        onSelect(context.asset);
        return;
      }

      const ctrlLike = event.ctrlKey || event.metaKey;
      const shift = event.shiftKey;
      if (dragMovedRef.current && !ctrlLike && !shift) {
        dragMovedRef.current = false;
        stopDragSelecting();
        event.preventDefault();
        return;
      }

      if (dragSelectingRef.current && !ctrlLike && !shift) {
        stopDragSelecting();
        event.preventDefault();
        return;
      }

      onBulkSelectionInteraction?.({
        assetId: context.asset.id,
        assetIndex: context.assetIndex,
        ctrlLike,
        shift,
        viaDrag: false
      });
      dragMovedRef.current = false;
      stopDragSelecting();
      event.preventDefault();
    },
    [onBulkSelectionInteraction, onSelect, readTileContext, selectionModeEnabled, stopDragSelecting]
  );

  return {
    isDragSelecting,
    handleGalleryWheel,
    handleTileMouseDown,
    handleTileMouseEnter,
    handleTileClick
  };
}
