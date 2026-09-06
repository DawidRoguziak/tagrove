import { useCallback, useEffect, useRef, useState } from "react";
import type { SelectedAsset } from "../types";

/** Identity resets drafts; canonical updates only replace untouched fields. */
export function useMediaGroupDraft(selected: SelectedAsset | null) {
  const [key, setKey] = useState("");
  const [order, setOrder] = useState("");
  const identity = useRef<number | null>(null);
  const dirty = useRef({ key: false, order: false });
  const id = selected?.id ?? null;
  const canonicalKey = selected?.media_group_key ?? "";
  const canonicalOrder =
    selected?.media_group_order == null ? "" : String(selected.media_group_order);
  useEffect(() => {
    if (identity.current !== id) {
      identity.current = id;
      dirty.current = { key: false, order: false };
    }
    if (!dirty.current.key) setKey(canonicalKey);
    if (!dirty.current.order) setOrder(canonicalOrder);
  }, [id, canonicalKey, canonicalOrder]);
  const editKey = useCallback((value: string) => {
    dirty.current.key = true;
    setKey(value);
  }, []);
  const editOrder = useCallback((value: string) => {
    dirty.current.order = true;
    setOrder(value);
  }, []);
  return {
    mediaGroupKeyEditor: key,
    mediaGroupOrderEditor: order,
    setMediaGroupKeyEditor: editKey,
    setMediaGroupOrderEditor: editOrder
  };
}
