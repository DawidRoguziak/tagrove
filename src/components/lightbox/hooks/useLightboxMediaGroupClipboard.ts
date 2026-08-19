import { useEffect, useRef, useState } from "react";

export function useLightboxMediaGroupClipboard(mediaGroupKeyEditor: string, selectedId: number | null) {
  const copyResetTimeoutRef = useRef<number | null>(null);
  const [groupCopyConfirmed, setGroupCopyConfirmed] = useState(false);

  useEffect(() => {
    setGroupCopyConfirmed(false);

    if (copyResetTimeoutRef.current !== null) {
      window.clearTimeout(copyResetTimeoutRef.current);
      copyResetTimeoutRef.current = null;
    }
  }, [mediaGroupKeyEditor, selectedId]);

  useEffect(() => {
    return () => {
      if (copyResetTimeoutRef.current !== null) {
        window.clearTimeout(copyResetTimeoutRef.current);
      }
    };
  }, []);

  const mediaGroupName = mediaGroupKeyEditor.trim();
  const hasClipboardApi =
    typeof navigator !== "undefined" && typeof navigator.clipboard?.writeText === "function";
  const canCopyMediaGroup = hasClipboardApi && mediaGroupName.length > 0;

  const copyMediaGroup = async () => {
    if (!canCopyMediaGroup) {
      return false;
    }

    try {
      await navigator.clipboard.writeText(mediaGroupName);
      setGroupCopyConfirmed(true);

      if (copyResetTimeoutRef.current !== null) {
        window.clearTimeout(copyResetTimeoutRef.current);
      }

      copyResetTimeoutRef.current = window.setTimeout(() => {
        copyResetTimeoutRef.current = null;
        setGroupCopyConfirmed(false);
      }, 1600);

      return true;
    } catch {
      setGroupCopyConfirmed(false);
      return false;
    }
  };

  return {
    mediaGroupName,
    canCopyMediaGroup,
    groupCopyConfirmed,
    copyMediaGroup
  };
}
