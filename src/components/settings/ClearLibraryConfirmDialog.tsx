import { UiButton } from "../UI/UiButton";
import { UiModal } from "../UI/UiModal";
import { useTranslation } from "react-i18next";

interface ClearLibraryConfirmDialogProps {
  open: boolean;
  isOperationLocked: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

export function ClearLibraryConfirmDialog({
  open,
  isOperationLocked,
  onClose,
  onConfirm
}: ClearLibraryConfirmDialogProps) {
  const { t } = useTranslation();

  if (!open) {
    return null;
  }

  return (
    <UiModal
      open={open}
      onClose={onClose}
      closeOnOverlayClick={!isOperationLocked}
      closeOnEscape={!isOperationLocked}
      size="small"
      labelledBy="clear-library-confirm-heading"
    >
      <div className="grid gap-2.5">
        <h3 id="clear-library-confirm-heading" className="m-0">
          {t("settings.dialogs.clearLibrary.heading")}
        </h3>
        <p className="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.dialogs.clearLibrary.description")}
        </p>
        <div className="flex items-center gap-2">
          <UiButton variant="danger" onClick={onConfirm} disabled={isOperationLocked}>
            {t("common.yes")}
          </UiButton>
          <UiButton onClick={onClose} disabled={isOperationLocked}>
            {t("common.no")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
