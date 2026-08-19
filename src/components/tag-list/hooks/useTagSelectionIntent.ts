import { useCallback, useEffect, useRef } from "react";
import type { Dispatch, SetStateAction } from "react";
import {
  applyDoubleSelectionAction,
  applySingleSelectionAction,
  type TagSelections
} from "../services/tagListSelectionService";

interface UseTagSelectionIntentOptions {
  singleClickDelayMs: number;
  onSelectionsChange: Dispatch<SetStateAction<TagSelections>>;
}

export function useTagSelectionIntent({
  singleClickDelayMs,
  onSelectionsChange
}: UseTagSelectionIntentOptions) {
  const pendingClickRef = useRef<{ tag: string; timeoutId: number } | null>(null);

  const clearPendingSingleClick = useCallback(() => {
    if (!pendingClickRef.current) {
      return;
    }

    window.clearTimeout(pendingClickRef.current.timeoutId);
    pendingClickRef.current = null;
  }, []);

  const flushPendingSingleClick = useCallback(
    (currentSelections: TagSelections): TagSelections => {
      if (!pendingClickRef.current) {
        return currentSelections;
      }

      const pendingTag = pendingClickRef.current.tag;
      clearPendingSingleClick();
      return applySingleSelectionAction(currentSelections, pendingTag);
    },
    [clearPendingSingleClick]
  );

  const handleSingleAction = useCallback(
    (tag: string) => {
      onSelectionsChange((previousSelections) => applySingleSelectionAction(previousSelections, tag));
    },
    [onSelectionsChange]
  );

  const handleDoubleAction = useCallback(
    (tag: string) => {
      onSelectionsChange((previousSelections) => applyDoubleSelectionAction(previousSelections, tag));
    },
    [onSelectionsChange]
  );

  const scheduleSingleAction = useCallback(
    (tag: string) => {
      if (pendingClickRef.current && pendingClickRef.current.tag !== tag) {
        const pendingTag = pendingClickRef.current.tag;
        clearPendingSingleClick();
        handleSingleAction(pendingTag);
      }

      clearPendingSingleClick();
      pendingClickRef.current = {
        tag,
        timeoutId: window.setTimeout(() => {
          pendingClickRef.current = null;
          handleSingleAction(tag);
        }, singleClickDelayMs)
      };
    },
    [clearPendingSingleClick, handleSingleAction, singleClickDelayMs]
  );

  useEffect(() => {
    return () => {
      clearPendingSingleClick();
    };
  }, [clearPendingSingleClick]);

  return {
    clearPendingSingleClick,
    flushPendingSingleClick,
    scheduleSingleAction,
    handleDoubleAction
  };
}
