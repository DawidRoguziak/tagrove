import { useCallback, useEffect, useRef, useState, type MouseEvent, type PointerEvent, type RefObject } from "react";

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
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [mediaGroupFailed, setMediaGroupFailed] = useState(false);
  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;

  useEffect(() => {
    if (selectedId === null) {
      return;
    }

    setTagsPanelOpen(false);
    setInfoPanelOpen(false);
    setDeleteConfirmOpen(false);
    setDeleteSubmitting(false);
    setDeleteError(null);
    setMediaGroupFailed(false);
  }, [selectedId]);

  const handleEnterFullscreen = useCallback(() => {
    setTagsPanelOpen(false);
    setInfoPanelOpen(false);
  }, []);

  const handleApplyMediaGroup = useCallback(() => {
    const nextKeyRaw = mediaGroupKeyEditor.trim();
    const nextKey = nextKeyRaw.length > 0 ? nextKeyRaw : null;

    const orderRaw = mediaGroupOrderEditor.trim();
    const parsedOrder = orderRaw ? Number(orderRaw) : null;
    if (parsedOrder !== null && !Number.isFinite(parsedOrder)) return;

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
    setDeleteError(null);
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
    deleteError,
    mediaGroupFailed,
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
