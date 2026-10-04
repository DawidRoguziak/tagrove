import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { UiButton } from "../../ui/controls/UiButton";
import { UiModal } from "../../ui/overlays/UiModal";
import { useLanguage } from "../../i18n/solid";
interface ClearLibraryConfirmDialogProps {
  open: boolean;
  isOperationLocked: boolean;
  onClose: () => void;
  onConfirm: () => void;
}
export function ClearLibraryConfirmDialog(props: ClearLibraryConfirmDialogProps) {
  const { t } = useLanguage();
  const visible = createModalSnapshot(
    () => props.open,
    () => ({ locked: props.isOperationLocked })
  );
  return (
    <UiModal
      open={props.open}
      onClose={props.onClose}
      closeOnOverlayClick={!props.isOperationLocked}
      closeOnEscape={!props.isOperationLocked}
      size="small"
      labelledBy="clear-library-confirm-heading"
    >
      <div class="grid gap-2.5">
        <h3 id="clear-library-confirm-heading" class="m-0">
          {t("settings.dialogs.clearLibrary.heading")}
        </h3>
        <p class="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.dialogs.clearLibrary.description")}
        </p>
        <div class="flex items-center gap-2">
          <UiButton variant="danger" onClick={props.onConfirm} disabled={visible().locked}>
            {t("common.yes")}
          </UiButton>
          <UiButton onClick={props.onClose} disabled={visible().locked}>
            {t("common.no")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
