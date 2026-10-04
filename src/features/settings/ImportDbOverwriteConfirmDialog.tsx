import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { UiButton } from "../../ui/controls/UiButton";
import { UiModal } from "../../ui/overlays/UiModal";
import { useLanguage } from "../../i18n/solid";
interface ImportDbOverwriteConfirmDialogProps {
  open: boolean;
  isOperationLocked: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}
export function ImportDbOverwriteConfirmDialog(props: ImportDbOverwriteConfirmDialogProps) {
  const { t } = useLanguage();
  const visible = createModalSnapshot(
    () => props.open,
    () => ({ locked: props.isOperationLocked })
  );
  return (
    <UiModal
      open={props.open}
      onClose={props.onCancel}
      closeOnOverlayClick={!props.isOperationLocked}
      closeOnEscape={!props.isOperationLocked}
      size="small"
      labelledBy="import-db-overwrite-confirm-heading"
    >
      <div class="grid gap-2.5">
        <h3 id="import-db-overwrite-confirm-heading" class="m-0">
          {t("common.confirm")}
        </h3>
        <p class="m-0 whitespace-pre-line text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.actions.dialogs.confirmDbImportOverwrite")}
        </p>
        <div class="flex items-center gap-2">
          <UiButton onClick={props.onCancel} disabled={visible().locked}>
            {t("common.cancel")}
          </UiButton>
          <UiButton variant="danger" onClick={props.onConfirm} disabled={visible().locked}>
            {t("common.confirm")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
