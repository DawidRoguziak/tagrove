import { useCallback, useEffect, useState, type MouseEvent, type PointerEvent, type RefObject } from "react";

interface UseLightboxModalHandlersOptions {
  selectedId: number | null;
  mediaGroupKeyEditor: string;
  mediaGroupOrderEditor: string;
  onSaveMediaGroup?: (next: { key: string | null; order: number | null }) => void | Promise<void>;
  onDeleteMedia: () => void | Promise<void>;
  onClose: () => void;
  tagPopoverContainerRef: RefObject<HTMLDivElement | null>;
  infoPopoverContainerRef: RefObject<HTMLDivElement | null>;
}

export function useLightboxModalHandlers({
  selectedId,
  mediaGroupKeyEditor,
  mediaGroupOrderEditor,
  onSaveMediaGroup,
  onDeleteMedia,
  onClose,
  tagPopoverContainerRef,
  infoPopoverContainerRef
}: UseLightboxModalHandlersOptions) {
  const [tagsPanelOpen, setTagsPanelOpen] = useState(false);
  const [infoPanelOpen, setInfoPanelOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deleteSubmitting, setDeleteSubmitting] = useState(false);

  useEffect(() => {
    if (selectedId === null) {
      return;
    }

    setTagsPanelOpen(false);
    setInfoPanelOpen(false);
    setDeleteConfirmOpen(false);
    setDeleteSubmitting(false);
  }, [selectedId]);

  const handleEnterFullscreen = useCallback(() => {
    setTagsPanelOpen(false);
    setInfoPanelOpen(false);
  }, []);

  const handleApplyMediaGroup = useCallback(() => {
    const nextKeyRaw = mediaGroupKeyEditor.trim();
    const nextKey = nextKeyRaw.length > 0 ? nextKeyRaw : null;

    const orderRaw = mediaGroupOrderEditor.trim();
    if (!orderRaw) {
      void onSaveMediaGroup?.({ key: nextKey, order: null });
      return;
    }

    const parsedOrder = Number(orderRaw);
    if (!Number.isFinite(parsedOrder)) {
      return;
    }

    void onSaveMediaGroup?.({ key: nextKey, order: parsedOrder });
  }, [mediaGroupKeyEditor, mediaGroupOrderEditor, onSaveMediaGroup]);

  const handleDeleteMedia = useCallback(async () => {
    if (deleteSubmitting) {
      return;
    }

    setDeleteSubmitting(true);
    try {
      await onDeleteMedia();
      setDeleteConfirmOpen(false);
    } finally {
      setDeleteSubmitting(false);
    }
  }, [deleteSubmitting, onDeleteMedia]);

  const handleShellPointerDownCapture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const targetNode = event.target;
      if (!(targetNode instanceof Node)) {
        return;
      }

      if (tagsPanelOpen && tagPopoverContainerRef.current && !tagPopoverContainerRef.current.contains(targetNode)) {
        setTagsPanelOpen(false);
      }

      if (infoPanelOpen && infoPopoverContainerRef.current && !infoPopoverContainerRef.current.contains(targetNode)) {
        setInfoPanelOpen(false);
      }
    },
    [infoPanelOpen, infoPopoverContainerRef, tagPopoverContainerRef, tagsPanelOpen]
  );

  const handleShellClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    event.stopPropagation();
  }, []);

  const handleToggleTagsPanel = useCallback(() => {
    setTagsPanelOpen((previous) => !previous);
    setInfoPanelOpen(false);
  }, []);

  const handleToggleInfoPanel = useCallback(() => {
    setInfoPanelOpen((previous) => !previous);
    setTagsPanelOpen(false);
  }, []);

  const handleOpenDeleteConfirm = useCallback(() => {
    setDeleteConfirmOpen(true);
    setTagsPanelOpen(false);
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
    tagsPanelOpen,
    infoPanelOpen,
    deleteConfirmOpen,
    deleteSubmitting,
    handleEnterFullscreen,
    handleApplyMediaGroup,
    handleShellPointerDownCapture,
    handleShellClick,
    handleToggleTagsPanel,
    handleToggleInfoPanel,
    handleOpenDeleteConfirm,
    handleCloseDeleteConfirm,
    handleConfirmDeleteMedia,
    handleCloseLightbox
  };
}
