import { UiButton } from "../UI/UiButton";
import { UiModal } from "../UI/UiModal";
import { useTranslation } from "react-i18next";

interface DuplicateDeleteConfirmDialogProps {
  open: boolean;
  count: number;
  deleteCount: number;
  isOperationLocked: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function DuplicateDeleteConfirmDialog({
  open,
  count,
  deleteCount,
  isOperationLocked,
  onCancel,
  onConfirm
}: DuplicateDeleteConfirmDialogProps) {
  const { t } = useTranslation();

  return (
    <UiModal
      open={open}
      onClose={onCancel}
      closeOnOverlayClick={!isOperationLocked}
      closeOnEscape={!isOperationLocked}
      size="small"
      labelledBy="duplicate-delete-confirm-heading"
    >
      <div className="grid gap-2.5">
        <h3 id="duplicate-delete-confirm-heading" className="m-0">
          {t("common.confirm")}
        </h3>
        <p className="m-0 whitespace-pre-line text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.actions.dialogs.confirmDuplicateChanges", {
            count,
            deleteCount
          })}
        </p>
        <div className="flex items-center gap-2">
          <UiButton onClick={onCancel} disabled={isOperationLocked}>
            {t("common.cancel")}
          </UiButton>
          <UiButton variant="danger" onClick={onConfirm} disabled={isOperationLocked}>
            {t("common.confirm")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
