import { UiButton } from "../UI/UiButton";
import { UiModal } from "../UI/UiModal";
import { useTranslation } from "react-i18next";

interface RemoveScanRootConfirmDialogProps {
  path: string | null;
  isOperationLocked: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function RemoveScanRootConfirmDialog({
  path,
  isOperationLocked,
  onCancel,
  onConfirm
}: RemoveScanRootConfirmDialogProps) {
  const { t } = useTranslation();

  if (!path) {
    return null;
  }

  return (
    <UiModal
      open
      onClose={onCancel}
      closeOnOverlayClick={!isOperationLocked}
      closeOnEscape={!isOperationLocked}
      size="medium"
      labelledBy="remove-scan-root-confirm-heading"
    >
      <div className="grid gap-2.5">
        <h3 id="remove-scan-root-confirm-heading" className="m-0">
          {t("settings.dialogs.removeScanRoot.heading")}
        </h3>
        <p className="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.dialogs.removeScanRoot.description")}
        </p>
        <p className="m-0 break-all rounded-[var(--radius-control)] border border-base-content/20 bg-base-200/50 px-2.5 py-2 text-xs text-[var(--text-muted)]">
          {path}
        </p>
        <div className="flex items-center gap-2">
          <UiButton variant="danger" onClick={onConfirm} disabled={isOperationLocked}>
            {t("common.remove")}
          </UiButton>
          <UiButton onClick={onCancel} disabled={isOperationLocked}>
            {t("common.cancel")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
