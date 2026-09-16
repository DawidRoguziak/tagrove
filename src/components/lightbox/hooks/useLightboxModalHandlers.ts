import { parseMediaGroupOrder } from "../services/parseMediaGroupOrder";
import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";

interface UseLightboxModalHandlersOptions {
  selectedId: number | null;
  mediaGroupKeyEditor: string;
  mediaGroupOrderEditor: string;
  onSaveMediaGroup?: (next: { key: string | null; order: number | null }) => void | Promise<void>;
  onDeleteMedia: () => void | Promise<void>;
  onClose: () => void;
}

export function useLightboxModalHandlers({
  selectedId,
  mediaGroupKeyEditor,
  mediaGroupOrderEditor,
  onSaveMediaGroup,
  onDeleteMedia,
  onClose
}: UseLightboxModalHandlersOptions) {
  const [infoPanelOpen, setInfoPanelOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deleteSubmitting, setDeleteSubmitting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [mediaGroupFailed, setMediaGroupFailed] = useState(false);
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;

  useEffect(() => {
    if (selectedId === null) {
      return;
    }

    setInfoPanelOpen(false);
    setDeleteConfirmOpen(false);
    setDeleteSubmitting(false);
    setDeleteError(null);
    setMediaGroupFailed(false);
  }, [selectedId]);

  const handleEnterFullscreen = useCallback(() => {
    setInfoPanelOpen(false);
  }, []);

  const handleApplyMediaGroup = useCallback(() => {
    const nextKeyRaw = mediaGroupKeyEditor.trim();
    const nextKey = nextKeyRaw.length > 0 ? nextKeyRaw : null;

    const parsedOrder = parseMediaGroupOrder(mediaGroupOrderEditor);
    if (parsedOrder === undefined) return;

    const requestedAssetId = selectedId;
    setMediaGroupFailed(false);
    void Promise.resolve(onSaveMediaGroup?.({ key: nextKey, order: parsedOrder })).catch(() => {
      if (selectedIdRef.current === requestedAssetId) setMediaGroupFailed(true);
    });
  }, [mediaGroupKeyEditor, mediaGroupOrderEditor, onSaveMediaGroup, selectedId]);

  const handleDeleteMedia = useCallback(async () => {
    if (deleteSubmitting) {
      return;
    }

    setDeleteSubmitting(true);
    setDeleteError(null);
    try {
      await onDeleteMedia();
      setDeleteConfirmOpen(false);
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : String(error));
    } finally {
      setDeleteSubmitting(false);
    }
  }, [deleteSubmitting, onDeleteMedia]);

  const handleShellClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    event.stopPropagation();
  }, []);

  const handleToggleInfoPanel = useCallback(() => {
    setInfoPanelOpen((previous) => !previous);
  }, []);

  const handleOpenDeleteConfirm = useCallback(() => {
    setDeleteConfirmOpen(true);
    setDeleteError(null);
    setInfoPanelOpen(false);
  }, []);

  const handleCloseDeleteConfirm = useCallback(() => {
    if (deleteSubmitting) {
      return;
    }

    setDeleteConfirmOpen(false);
  }, [deleteSubmitting]);

  const handleConfirmDeleteMedia = useCallback(() => {
    void handleDeleteMedia();
  }, [handleDeleteMedia]);

  const handleCloseLightbox = useCallback(() => {
    onClose();
  }, [onClose]);

  return {
    infoPanelOpen,
    deleteConfirmOpen,
    deleteSubmitting,
    deleteError,
    mediaGroupFailed,
    handleEnterFullscreen,
    handleApplyMediaGroup,
    handleShellClick,
    handleToggleInfoPanel,
    handleOpenDeleteConfirm,
    handleCloseDeleteConfirm,
    handleConfirmDeleteMedia,
    handleCloseLightbox
  };
}
